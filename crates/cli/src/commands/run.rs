//! `yunta run <workflow>`: resolve config, check, freeze the manifest,
//! prepare the run's isolated working tree, create the run and execute
//! it there.
//!
//! The run id is a ULID from the shell's id source; the engine mints
//! only the ids of the runs this one gives birth to, through the same
//! injected source.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use yunta_adapters::MOCK_ID;
use yunta_core::{AdapterId, Clock, IdSource, Isolation, Manifest, ModeName, RunId, Workflow};
use yunta_engine::{RunEnv, RunReport, RunTerminal, DEFAULT_MAX_RETRIES};
use yunta_storage::AsyncStorage;

use super::status::progress_summary;
use crate::context::Context;
use crate::error::{warn, CliError, Outcome};
use crate::json::SCHEMA_VERSION;
use crate::load_yaml;

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

#[allow(clippy::too_many_arguments)]
pub async fn run(
    workflow_path: &Path,
    raw_inputs: &[String],
    adapter: Option<&AdapterId>,
    fixture: Option<&Path>,
    mode: Option<&ModeName>,
    follow: bool,
    detach: bool,
    json: bool,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let storage = ctx.async_storage().await?;

    // `--adapter mock --fixture <path>` runs against a scripted fixture:
    // no real adapter is constructed or probed. Any other `--adapter`
    // is an override every role resolves through. Detected before the
    // workflow is even loaded, so a misuse fails fast.
    let mock_fixture = match (adapter, fixture) {
        (Some(id), Some(path)) if *id == MOCK_ID => Some(path),
        (Some(id), None) if *id == MOCK_ID => {
            return Err(CliError::msg(
                "`--adapter mock` runs the workflow against a scripted fixture — pass \
                 `--fixture <path>` (the format a `.yunta/tests/` fixture uses), or write a \
                 case and run `yunta test`",
            ));
        }
        (_, Some(_)) => {
            return Err(CliError::msg(
                "`--fixture` only applies together with `--adapter mock`",
            ));
        }
        _ => None,
    };

    // Detach is the control plane's own shape: create the run, hand it to
    // a fully independent `yunta resume`, and return its id without
    // waiting — never with a mock fixture, which is an inline test tool.
    // The exact `start_detached` the MCP `run_workflow` tool calls.
    if detach && mock_fixture.is_none() {
        let run_id =
            start_detached(&ctx, &storage, workflow_path, raw_inputs, adapter, mode).await?;
        if json {
            return crate::json::print_json(&RunJson::detached(&run_id));
        }
        println!("run {run_id}: detached, driving forward independently");
        return Ok(Outcome::Success);
    }

    let (workflow_path, workflow) = resolve_and_check(&ctx, workflow_path)?;
    let adapter_override = adapter.filter(|id| **id != MOCK_ID).cloned();
    let real_adapters = if mock_fixture.is_some() {
        HashMap::new()
    } else {
        super::real_adapters(&ctx.project.config)
    };
    if mock_fixture.is_none() {
        super::refuse_unrunnable(&workflow, &real_adapters)?;
        if let Some(name) = adapter {
            validate_adapter_flag(name, &real_adapters).map_err(CliError::msg)?;
        }
        super::probe_or_refuse(&real_adapters).await?;
    }

    let manifest = build_frozen_manifest(&ctx, &workflow, &workflow_path, raw_inputs)?;

    // Informative, never blocking — suppressed under `--json` so the
    // document is the only thing on stdout. The history a run's own log
    // joins once it finishes; a log that cannot be read is an empty
    // history here, exactly as it is for `stats`.
    if !json {
        let history = {
            let runs_root = ctx.project.runs_root.clone();
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
    }

    let Prepared {
        run_id,
        run_dir,
        worktree,
    } = create_run_from(&ctx, &storage, &manifest, mode).await?;
    if !json {
        println!("run {run_id}: created at {}", run_dir.display());
    }

    let adapters = match mock_fixture {
        Some(path) => {
            let mock =
                super::test::load_mock_fixture(path, &run_dir, &worktree).map_err(CliError::msg)?;
            super::test::mock_adapters(&ctx.project.config, mock)
        }
        None => real_adapters,
    };

    // `--follow`'s progress stream would interleave with the JSON
    // document; under `--json` the final DTO is the whole story.
    let follower = (follow && !json)
        .then(|| spawn_follower(storage.clone(), run_id.clone(), manifest.clone()));

    let forge = super::real_forge(&manifest.config);
    let root_cancel = super::cancel_on_ctrl_c();
    let outcome = yunta_engine::execute_run(RunEnv {
        run_id: &run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &worktree,
        adapters: &adapters,
        storage: &storage,
        clock: std::sync::Arc::new(ctx.clock),
        ids: &ctx.ids,
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
            let (run_id, manifest, _worktree, report) = super::promote::drive_promotions(
                &super::promote::PromotionEnv {
                    cwd: &ctx.cwd,
                    project: &ctx.project,
                    storage: &storage,
                    ids: &ctx.ids,
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
            .map_err(CliError::msg)?;
            // Only a *finished* run releases isolation `none`'s lock — a
            // paused run expects a future `resume` on the same checkout,
            // which is the same logical run, not a second concurrent one.
            // A user cancellation also releases `none`'s lock — the
            // engine process is exiting, and a Ctrl-C is designed to
            // leave nothing held.
            if matches!(report.terminal, RunTerminal::Finished) || root_cancel.is_cancelled() {
                yunta_engine::release_worktree(&ctx.cwd, manifest.isolation).await?;
            }
            if json {
                crate::json::print_json(&RunJson::from_report(&run_id, &report))?;
                Ok(match report.terminal {
                    RunTerminal::Finished => Outcome::Success,
                    _ => Outcome::Reported,
                })
            } else {
                Ok(super::report_outcome(run_id.as_str(), &report))
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// A workflow reference resolved to its file and loaded, refused if it
/// fails `yunta check` — the shared front of `yunta run` and the control
/// plane's `run_workflow`, so both reach the same workflow the same way: a
/// bare catalog name (no extension) resolves through the repo catalog then
/// a publisher's vendored packs (`acme/review`); anything with an
/// extension is taken as a literal path.
fn resolve_and_check(ctx: &Context, workflow_path: &Path) -> Result<(PathBuf, Workflow), CliError> {
    let resolved = if workflow_path.extension().is_none() {
        yunta_engine::resolve_workflow(&ctx.cwd, &workflow_path.to_string_lossy())?.path
    } else {
        workflow_path.to_path_buf()
    };
    let workflow: Workflow = load_yaml(&resolved, "workflow")?;
    super::check_or_refuse(&workflow, &ctx.project.config, &resolved)?;
    Ok((resolved, workflow))
}

/// Builds the run's manifest and freezes its state roots, which
/// [`FrozenPaths::new`](yunta_core::FrozenPaths::new) requires to be
/// absolute — `resume`/`status`/`gc` read these back from any directory,
/// so a relative root (a relative `paths.*` or `YUNTA_HOME`) is refused
/// here, naming it, rather than silently rooted at the invocation's cwd.
fn build_frozen_manifest(
    ctx: &Context,
    workflow: &Workflow,
    workflow_path: &Path,
    raw_inputs: &[String],
) -> Result<Manifest, CliError> {
    let provided_inputs = parse_inputs(raw_inputs).map_err(CliError::msg)?;
    let workflow_dir = workflow_path.parent().unwrap_or(Path::new("."));
    let mut manifest = yunta_engine::build_manifest(
        workflow,
        &ctx.project.config,
        workflow_dir,
        &ctx.cwd,
        &provided_inputs,
    )?;
    manifest.paths = Some(yunta_core::FrozenPaths::new(
        ctx.project.runs_root.clone(),
        ctx.project.worktrees_root.clone(),
    )?);
    Ok(manifest)
}

/// A created run's identity and the paths it lives at — what both the
/// executing and the detaching path need after `create_run`.
struct Prepared {
    run_id: RunId,
    run_dir: PathBuf,
    worktree: PathBuf,
}

/// Enforces the soft concurrency cap, mints the run id from the injected
/// clock, prepares the isolation worktree and creates the run — the
/// shared create step of an executed run and a detached one. Prints
/// nothing of its own: the caller reports what it made.
async fn create_run_from(
    ctx: &Context,
    storage: &AsyncStorage,
    manifest: &Manifest,
    mode: Option<&ModeName>,
) -> Result<Prepared, CliError> {
    // A soft budget, not a safety limit — best-effort by design (two
    // simultaneous `yunta run` invocations can both pass the count),
    // checked before anything is created so the refusal costs nothing.
    if let Some(cap) = manifest
        .config
        .limits
        .as_ref()
        .and_then(|limits| limits.max_concurrent_runs)
    {
        let active = count_non_terminal_runs(storage).await?;
        if active >= cap as usize {
            return Err(CliError::msg(format!(
                "{active} run(s) are still active and `limits.max_concurrent_runs` \
                 is {cap} — resume or cancel one (`yunta list` names them) before starting \
                 another"
            )));
        }
    }

    let run_id = ctx.ids.mint_run_id(ctx.clock.now());
    let worktree = match manifest.isolation {
        Isolation::Worktree => ctx.project.worktrees_root.join(run_id.as_str()),
        Isolation::None => ctx.cwd.clone(),
    };
    match yunta_engine::prepare_worktree(
        &ctx.cwd,
        &worktree,
        &manifest.base_commit,
        &format!("yunta/{run_id}"),
        manifest.isolation,
    )
    .await?
    {
        yunta_engine::WorktreePrepared::Ready => {}
        yunta_engine::WorktreePrepared::StoleStaleLock { dead_pid } => {
            warn(format!(
                "this checkout's isolation lock belonged to a dead process \
                 (pid {dead_pid}) — taking it over"
            ));
        }
    }

    // An explicit `--mode` is used as given (`create_run` itself refuses
    // an unknown name); omitted with `modes:` declared defaults to the
    // *first* declared mode — promotion only ever escalates forward, so
    // starting at the floor is the one default that can never need walking
    // back. A workflow with no `modes:` at all keeps running everything,
    // unaffected.
    let resolved_mode = mode.cloned().unwrap_or_else(|| {
        manifest
            .workflow
            .modes
            .as_ref()
            .and_then(|modes| modes.keys().next())
            .cloned()
            .unwrap_or_default()
    });

    let run_dir = yunta_engine::create_run(
        yunta_engine::CreateRunParams {
            run_id: &run_id,
            manifest,
            runs_root: &ctx.project.runs_root,
            mode: &resolved_mode,
            promoted_from: None,
            artifacts: &[],
        },
        storage,
        &ctx.clock,
    )
    .await?;

    Ok(Prepared {
        run_id,
        run_dir,
        worktree,
    })
}

/// Creates a run for a real (non-mock) workflow and hands it to a detached
/// `yunta resume`, returning its id without waiting — the shared core of
/// `yunta run --detach` and the MCP `run_workflow` tool, so both resolve,
/// check, probe, freeze and create a run identically. Prints nothing: the
/// caller decides how to report the id (a line, or a DTO).
pub(crate) async fn start_detached(
    ctx: &Context,
    storage: &AsyncStorage,
    workflow_path: &Path,
    raw_inputs: &[String],
    adapter: Option<&AdapterId>,
    mode: Option<&ModeName>,
) -> Result<RunId, CliError> {
    let (workflow_path, workflow) = resolve_and_check(ctx, workflow_path)?;
    let adapters = super::real_adapters(&ctx.project.config);
    super::refuse_unrunnable(&workflow, &adapters)?;
    if let Some(name) = adapter {
        validate_adapter_flag(name, &adapters).map_err(CliError::msg)?;
    }
    super::probe_or_refuse(&adapters).await?;

    let manifest = build_frozen_manifest(ctx, &workflow, &workflow_path, raw_inputs)?;
    let prepared = create_run_from(ctx, storage, &manifest, mode).await?;
    super::spawn_detached_resume(&prepared.run_dir, prepared.run_id.as_str(), &ctx.cwd)
        .await
        .map_err(|source| {
            CliError::io(
                "spawn a detached",
                format!("`yunta resume {}`", prepared.run_id),
                source,
            )
        })?;
    Ok(prepared.run_id)
}

/// The outcome of a `yunta run` as the versioned JSON `--json` prints and
/// the control plane can hand back — the run's id and how it ended, or
/// that it detached to run on independently.
#[derive(serde::Serialize)]
struct RunJson {
    schema_version: u32,
    run_id: String,
    /// `detached`, or the run's terminal: `finished`, `paused`,
    /// `promoted`.
    outcome: &'static str,
}

impl RunJson {
    fn detached(run_id: &RunId) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            run_id: run_id.to_string(),
            outcome: "detached",
        }
    }

    fn from_report(run_id: &RunId, report: &RunReport) -> Self {
        let outcome = match &report.terminal {
            RunTerminal::Finished => "finished",
            RunTerminal::Paused { .. } => "paused",
            RunTerminal::Promoted { .. } => "promoted",
        };
        Self {
            schema_version: SCHEMA_VERSION,
            run_id: run_id.to_string(),
            outcome,
        }
    }
}
