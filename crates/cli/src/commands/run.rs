//! `yunta run <workflow>`: resolve config, check, freeze the manifest,
//! prepare the run's isolated working tree, create the run and execute
//! it there.
//!
//! The run id is a ULID from the shell's id source; the engine mints
//! only the ids of the runs this one gives birth to, through the same
//! injected source.

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use yunta_adapters::MOCK_ID;
use yunta_core::{
    AdapterId, Clock, IdSource, Isolation, Manifest, ModeName, RunId, SystemClock, SystemIdSource,
    Workflow,
};
use yunta_engine::{RunEnv, RunTerminal, DEFAULT_MAX_RETRIES};
use yunta_storage::AsyncStorage;

use super::status::progress_summary;
use crate::load_yaml;
use crate::project;

/// Parses `--input name=value` entries into the raw map
/// `yunta_engine::resolve_inputs` validates against the workflow's own
/// `inputs:` — this function only enforces the *syntax* of the flag
/// (exactly one `=`, non-empty name); everything about whether
/// a name is declared, required, or well-typed is `resolve_inputs`'s
/// job, not this one's, so the two error paths never disagree about who
/// owns which rule.
fn parse_inputs(raw: &[String]) -> Result<HashMap<String, String>, String> {
    let mut inputs = HashMap::new();
    for entry in raw {
        let (name, value) = entry
            .split_once('=')
            .ok_or_else(|| format!("--input `{entry}` must have the form `name=value`"))?;
        if name.is_empty() {
            return Err(format!("--input `{entry}` has an empty name"));
        }
        if inputs.insert(name.to_string(), value.to_string()).is_some() {
            return Err(format!("--input `{name}` was given more than once"));
        }
    }
    Ok(inputs)
}

/// `--adapter <name>` names a real adapter every session runs on; it
/// must be one `real_adapters` constructed from the config. `mock` is
/// handled before this: it needs a fixture, never a real binary.
fn validate_adapter_flag(
    name: &AdapterId,
    adapters: &HashMap<AdapterId, std::sync::Arc<dyn yunta_adapters::Adapter>>,
) -> Result<(), String> {
    if !adapters.contains_key(name) {
        return Err(format!(
            "unknown adapter `{name}` — this binary can run: {}",
            if adapters.is_empty() {
                "(none configured — `runners:` names no adapter this build supports)".to_string()
            } else {
                let mut names: Vec<&str> = adapters.keys().map(AdapterId::as_str).collect();
                names.sort();
                names.join(", ")
            }
        ));
    }
    Ok(())
}

/// `run --follow`: a background task that re-reads the run's own event
/// log every 500ms and prints `progress_summary` whenever it changes.
/// This polls rather than subscribing to a real push stream, since
/// `yunta-storage` exposes no such subscription mechanism — a
/// deliberately narrow, ~5-method interface; the *content* printed is
/// identical either way, only the delivery latency (bounded by the
/// poll interval) differs from a true stream. Reads through its own
/// clone of the run's log handle: every poll is its own read-only
/// connection on a blocking thread, which WAL mode (set by
/// `Storage::open`) keeps safe beside the writer `execute_run` itself
/// is using.
fn spawn_follower(
    storage: AsyncStorage,
    run_id: RunId,
    manifest: Manifest,
) -> (
    tokio::task::JoinHandle<()>,
    std::sync::Arc<tokio::sync::Notify>,
) {
    let stop = std::sync::Arc::new(tokio::sync::Notify::new());
    let stop_follower = stop.clone();
    let handle = tokio::spawn(async move {
        let mut last = String::new();
        loop {
            tokio::select! {
                _ = stop_follower.notified() => return,
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
            let Ok(events) = storage.events_for_run(run_id.clone()).await else {
                continue;
            };
            if events.is_empty() {
                continue;
            }
            let summary = progress_summary(&events, &manifest);
            if summary != last {
                println!("run {run_id}: {summary}");
                last = summary;
            }
        }
    });
    (handle, stop)
}

/// Every run in storage with events but no `run_finished` — paused runs
/// hold a slot (they expect a `resume`), finished ones never do.
async fn count_non_terminal_runs(
    storage: &AsyncStorage,
) -> Result<usize, yunta_storage::StorageError> {
    let mut active = 0;
    for run_id in storage.list_runs().await?.into_iter().map(|run| run.run_id) {
        let events = storage.events_for_run(run_id).await?;
        let finished = events.iter().any(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::RunFinished(_))
            )
        });
        if !events.is_empty() && !finished {
            active += 1;
        }
    }
    Ok(active)
}

pub async fn run(
    workflow_path: &Path,
    raw_inputs: &[String],
    adapter: Option<&AdapterId>,
    fixture: Option<&Path>,
    mode: Option<&ModeName>,
    follow: bool,
    detach: bool,
) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let project = match project::resolve(&cwd) {
        Ok(project) => project,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let storage = match AsyncStorage::open(&project.storage_path).await {
        Ok(storage) => storage,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // A bare catalog name (no `.yaml`/`.yml` extension — every real
    // workflow file in this codebase's own convention has one) resolves
    // through the repo catalog, then a publisher's vendored packs
    // (e.g. `yunta run acme/review`); anything with an extension stays
    // a literal path, today's behavior unchanged.
    let workflow_path: std::path::PathBuf = if workflow_path.extension().is_none() {
        match yunta_engine::resolve_workflow(&cwd, &workflow_path.to_string_lossy()) {
            Ok(resolved) => resolved.path,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        workflow_path.to_path_buf()
    };
    let workflow_path = workflow_path.as_path();

    let workflow: Workflow = match load_yaml(workflow_path, "workflow") {
        Ok(w) => w,
        Err(code) => return code,
    };
    if let Err(code) = super::check_or_refuse(&workflow, &project.config, workflow_path) {
        return code;
    }
    // `--adapter mock --fixture <path>` runs against a scripted fixture:
    // no real adapter is constructed or probed. Any other `--adapter`
    // is an override every role resolves through.
    let mock_fixture = match (adapter, fixture) {
        (Some(id), Some(path)) if *id == MOCK_ID => Some(path),
        (Some(id), None) if *id == MOCK_ID => {
            eprintln!(
                "error: `--adapter mock` runs the workflow against a scripted fixture — pass \
                 `--fixture <path>` (the format a `.yunta/tests/` fixture uses), or write a \
                 case and run `yunta test`"
            );
            return ExitCode::FAILURE;
        }
        (_, Some(_)) => {
            eprintln!("error: `--fixture` only applies together with `--adapter mock`");
            return ExitCode::FAILURE;
        }
        _ => None,
    };
    let adapter_override = adapter.filter(|id| **id != MOCK_ID).cloned();
    let real_adapters = if mock_fixture.is_some() {
        HashMap::new()
    } else {
        super::real_adapters(&project.config)
    };
    if mock_fixture.is_none() {
        if let Err(code) = super::refuse_unrunnable(&workflow, &real_adapters) {
            return code;
        }
        if let Some(name) = adapter {
            if let Err(e) = validate_adapter_flag(name, &real_adapters) {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
        if let Err(code) = super::probe_or_refuse(&real_adapters).await {
            return code;
        }
    }

    let provided_inputs = match parse_inputs(raw_inputs) {
        Ok(inputs) => inputs,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let workflow_dir = workflow_path.parent().unwrap_or(Path::new("."));
    let mut manifest = match yunta_engine::build_manifest(
        &workflow,
        &project.config,
        workflow_dir,
        &cwd,
        &provided_inputs,
    ) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Freeze the resolved state roots, absolute, so a later `paths.*`
    // change can never lose this run — `resume`/`status` read these
    // from the manifest, not the then-current config.
    manifest.paths = Some(yunta_core::FrozenPaths {
        runs_root: std::path::absolute(&project.runs_root)
            .unwrap_or_else(|_| project.runs_root.clone()),
        worktrees_root: std::path::absolute(&project.worktrees_root)
            .unwrap_or_else(|_| project.worktrees_root.clone()),
    });

    // Informative, never blocking — the history a run's own log will
    // later join once it finishes; a log that cannot be read is an
    // empty history here, exactly as it is for `stats`.
    let history = {
        let runs_root = project.runs_root.clone();
        let workflow_name = workflow.name.clone();
        storage
            .blocking("collect the workflow's history", move |storage| {
                Ok(super::stats::collect_history(
                    &runs_root,
                    storage,
                    &workflow_name,
                ))
            })
            .await
            .unwrap_or_default()
    };
    let estimation = yunta_engine::prior_estimation(&history);
    if let Some(estimation) = &estimation {
        println!("{}", super::stats::format_estimation_line(estimation));
    }
    // Budget-vs-p90, informative and never blocking.
    if let Some(warning) = yunta_engine::budget_p90_warning(
        manifest
            .config
            .limits
            .as_ref()
            .and_then(|limits| limits.max_tokens_per_run),
        estimation.as_ref(),
    ) {
        println!("{warning}");
    }

    // A soft budget, not a safety limit — best-effort by design (two
    // simultaneous `yunta run` invocations can both pass the count),
    // checked before anything is created so the refusal costs nothing.
    if let Some(cap) = manifest
        .config
        .limits
        .as_ref()
        .and_then(|limits| limits.max_concurrent_runs)
    {
        match count_non_terminal_runs(&storage).await {
            Ok(active) if active >= cap as usize => {
                eprintln!(
                    "error: {active} run(s) are still active and `limits.max_concurrent_runs` \
                     is {cap} — resume or cancel one (`yunta list` names them) before starting \
                     another"
                );
                return ExitCode::FAILURE;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    let clock = SystemClock;
    let ids = SystemIdSource;
    let run_id = ids.mint_run_id(clock.now());

    let worktree = match manifest.isolation {
        Isolation::Worktree => project.worktrees_root.join(run_id.as_str()),
        Isolation::None => cwd.clone(),
    };
    match yunta_engine::prepare_worktree(
        &cwd,
        &worktree,
        &manifest.base_commit,
        &format!("yunta/{run_id}"),
        manifest.isolation,
    )
    .await
    {
        Ok(yunta_engine::WorktreePrepared::Ready) => {}
        Ok(yunta_engine::WorktreePrepared::StoleStaleLock { dead_pid }) => {
            eprintln!(
                "warning: this checkout's isolation lock belonged to a dead process \
                 (pid {dead_pid}) — taking it over"
            );
        }
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    }

    // An explicit `--mode` is used as given (`create_run` itself
    // refuses an unknown name); omitted with `modes:` declared
    // defaults to the *first* declared mode — promotion only ever
    // escalates forward, so starting at the floor is the one default
    // that can never need walking back. A workflow with no
    // `modes:` at all keeps running everything, unaffected.
    let resolved_mode = mode.cloned().unwrap_or_else(|| {
        manifest
            .workflow
            .modes
            .as_ref()
            .and_then(|modes| modes.keys().next())
            .cloned()
            .unwrap_or_default()
    });

    let run_dir = match yunta_engine::create_run(
        yunta_engine::CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &project.runs_root,
            mode: &resolved_mode,
            promoted_from: None,
            artifacts: &[],
        },
        &storage,
        &clock,
    )
    .await
    {
        Ok(run_dir) => run_dir,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("run {run_id}: created at {}", run_dir.display());

    let adapters = match mock_fixture {
        Some(path) => match super::test::load_mock_fixture(path, &run_dir, &worktree) {
            Ok(mock) => super::test::mock_adapters(&project.config, mock),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => real_adapters,
    };

    // `run_workflow`'s own async pattern — create synchronously (fast,
    // no agent I/O yet), then hand off to a fully independent
    // `yunta resume` and return.
    if detach {
        if let Err(e) = super::spawn_detached_resume(&run_dir, run_id.as_str(), &cwd) {
            eprintln!("error: cannot spawn a detached `yunta resume {run_id}`: {e}");
            return ExitCode::FAILURE;
        }
        println!("run {run_id}: detached, driving forward independently");
        return ExitCode::SUCCESS;
    }

    let follower =
        follow.then(|| spawn_follower(storage.clone(), run_id.clone(), manifest.clone()));

    let forge = super::real_forge(&manifest.config);
    let root_cancel = super::cancel_on_ctrl_c();
    let outcome = yunta_engine::execute_run(RunEnv {
        run_id: &run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &worktree,
        adapters: &adapters,
        storage: &storage,
        clock: std::sync::Arc::new(SystemClock),
        ids: &ids,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &crate::human_interaction::ConsoleInteraction,
        forge: forge.as_deref(),
        cancel: Some(&root_cancel),
        adapter_override: adapter_override.as_ref(),
    })
    .await;

    if let Some((handle, stop)) = follower {
        stop.notify_one();
        let _ = handle.await;
    }

    match outcome {
        Ok(report) => {
            // Chases every `Promoted` terminal to its actual end before
            // anything downstream (release, report) looks at it — see
            // `promote.rs`'s own doc comment for why this can't happen
            // inside `execute_run` itself.
            let (run_id, manifest, _worktree, report) = match super::promote::drive_promotions(
                &super::promote::PromotionEnv {
                    cwd: &cwd,
                    project: &project,
                    storage: &storage,
                    ids: &ids,
                    adapters: &adapters,
                    forge: forge.as_deref(),
                    cancel: Some(&root_cancel),
                },
                run_id,
                manifest,
                worktree,
                report,
            )
            .await
            {
                Ok(chained) => chained,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            // Only a *finished* run releases isolation `none`'s lock — a
            // paused run expects a future `resume` on the same checkout,
            // which is the same logical run, not a second concurrent one.
            // A user cancellation also releases `none`'s lock — the
            // engine process is exiting, and a Ctrl-C is designed to
            // leave nothing held.
            if matches!(report.terminal, RunTerminal::Finished) || root_cancel.is_cancelled() {
                if let Err(e) = yunta_engine::release_worktree(&cwd, manifest.isolation).await {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            }
            super::report_outcome(run_id.as_str(), &report)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
