//! `yunta run <workflow>` (T7.1): resolve config, check, freeze the
//! manifest, prepare the run's isolated working tree (T4.2, §7.3),
//! create the run and execute it there.
//!
//! The run id comes from the wall clock + pid — the shell may use
//! entropy, the engine never does.

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use yunta_core::{Isolation, Manifest, RunId, SystemClock, Workflow};
use yunta_engine::{RunTerminal, DEFAULT_MAX_RETRIES};
use yunta_storage::Storage;

use super::status::progress_summary;
use crate::load_yaml;
use crate::project;

/// Parses `--input name=value` entries into the raw map
/// `yunta_engine::resolve_inputs` validates against the workflow's own
/// `inputs:` (T1.5, §2.3) — this function only enforces the *syntax* of
/// the flag (exactly one `=`, non-empty name); everything about whether
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

/// `--adapter <name>` (T7.1) picks which adapter a run's sessions use.
/// `mock` is a legitimate adapter id (Spec Adapter §6) but its fixtures
/// are `yunta test`'s territory (`docs/m0-status.md`'s T6.1 entry): a
/// real `run` has no `.yunta/tests/` case to script it from, so naming
/// it here degrades explicitly instead of spawning a mock with nothing
/// to simulate. Any other name must be one `real_adapters` would
/// actually construct — `claude-code` and `codex`, today.
fn validate_adapter_flag(
    name: &str,
    adapters: &HashMap<String, std::sync::Arc<dyn yunta_adapters::Adapter>>,
) -> Result<(), String> {
    if name == "mock" {
        return Err(
            "`--adapter mock` has no fixture to run without a `.yunta/tests/` case — \
             use `yunta test` instead"
                .to_string(),
        );
    }
    if !adapters.contains_key(name) {
        return Err(format!(
            "unknown adapter `{name}` — this binary can run: {}",
            if adapters.is_empty() {
                "(none configured — `runners:` names no adapter this build supports)".to_string()
            } else {
                let mut names: Vec<&str> = adapters.keys().map(String::as_str).collect();
                names.sort();
                names.join(", ")
            }
        ));
    }
    Ok(())
}

/// `run --follow` (§8.5): a background task that re-reads the run's own
/// event log every 500ms and prints `progress_summary` whenever it
/// changes. §8.5's own text says "consumiendo el stream de eventos" —
/// this recorte polls rather than subscribing to a real push stream,
/// since `yunta-storage` exposes no such subscription mechanism (D53:
/// a ~5-method interface, on purpose); the *content* printed is
/// identical either way, only the delivery latency (bounded by the
/// poll interval) differs from a true stream. Opens its own `Storage`
/// handle onto the same SQLite file — WAL mode (already set by
/// `Storage::open`) is exactly what makes a second, read-only
/// connection safe to run concurrently with the writer `execute_run`
/// itself is using.
fn spawn_follower(
    storage_path: std::path::PathBuf,
    run_id: RunId,
    manifest: Manifest,
) -> (
    tokio::task::JoinHandle<()>,
    std::sync::Arc<tokio::sync::Notify>,
) {
    let stop = std::sync::Arc::new(tokio::sync::Notify::new());
    let stop_follower = stop.clone();
    let handle = tokio::spawn(async move {
        let storage = match Storage::open(&storage_path) {
            Ok(storage) => storage,
            Err(_) => return, // `run` itself already opened this path fine.
        };
        let mut last = String::new();
        loop {
            tokio::select! {
                _ = stop_follower.notified() => return,
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
            let Ok(events) = storage.events_for_run(&run_id) else {
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

pub async fn run(
    workflow_path: &Path,
    raw_inputs: &[String],
    adapter: Option<&str>,
    mode: Option<&str>,
    follow: bool,
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

    let storage = match Storage::open(&project.storage_path) {
        Ok(storage) => storage,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let workflow: Workflow = match load_yaml(workflow_path, "workflow") {
        Ok(w) => w,
        Err(code) => return code,
    };
    if let Err(code) = super::check_or_refuse(&workflow, &project.config) {
        return code;
    }
    let adapters = super::real_adapters(&project.config);
    if let Err(code) = super::refuse_unrunnable(&workflow, &adapters) {
        return code;
    }
    if let Some(name) = adapter {
        if let Err(e) = validate_adapter_flag(name, &adapters) {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    }
    if let Err(code) = super::probe_or_refuse(&adapters).await {
        return code;
    }

    let provided_inputs = match parse_inputs(raw_inputs) {
        Ok(inputs) => inputs,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let workflow_dir = workflow_path.parent().unwrap_or(Path::new("."));
    let manifest = match yunta_engine::build_manifest(
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

    // §8.6: informative, never blocking — the history a run's own log
    // will later join once it finishes.
    let history = super::stats::collect_history(&project, &storage, &workflow.name);
    if let Some(estimation) = yunta_engine::prior_estimation(&history) {
        println!("{}", super::stats::format_estimation_line(&estimation));
    }

    let run_id = RunId::from(format!(
        "run-{}-{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S"),
        std::process::id()
    ));

    let worktree = match manifest.isolation {
        Isolation::Worktree => project.worktrees_root.join(run_id.as_str()),
        Isolation::None => cwd.clone(),
    };
    if let Err(e) = yunta_engine::prepare_worktree(
        &cwd,
        &worktree,
        &manifest.base_commit,
        &format!("yunta/{run_id}"),
        manifest.isolation,
    )
    .await
    {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    let clock = SystemClock;

    // §10.1/D44: an explicit `--mode` is used as given (`create_run`
    // itself refuses an unknown name); omitted with `modes:` declared
    // defaults to the *first* declared mode — promotion (§10.2) only
    // ever escalates forward, so starting at the floor is the one
    // default that can never need walking back. A workflow with no
    // `modes:` at all keeps running everything, unaffected.
    let resolved_mode = mode.map(str::to_string).unwrap_or_else(|| {
        manifest
            .workflow
            .modes
            .as_ref()
            .and_then(|modes| modes.keys().next())
            .cloned()
            .unwrap_or_else(|| "default".to_string())
    });

    let run_dir = match yunta_engine::create_run(
        &run_id,
        &manifest,
        &project.runs_root,
        &storage,
        &clock,
        &resolved_mode,
    ) {
        Ok(run_dir) => run_dir,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("run {run_id}: created at {}", run_dir.display());

    let follower = follow.then(|| {
        spawn_follower(
            project.storage_path.clone(),
            run_id.clone(),
            manifest.clone(),
        )
    });

    let forge = super::real_forge(&manifest.config);
    let outcome = yunta_engine::execute_run(
        &run_id,
        &manifest,
        &run_dir,
        &worktree,
        &adapters,
        &storage,
        &clock,
        DEFAULT_MAX_RETRIES,
        &crate::human_interaction::ConsoleInteraction,
        forge.as_deref(),
    )
    .await;

    if let Some((handle, stop)) = follower {
        stop.notify_one();
        let _ = handle.await;
    }

    match outcome {
        Ok(report) => {
            // Only a *finished* run releases isolation `none`'s lock — a
            // paused run expects a future `resume` on the same checkout,
            // which is the same logical run, not a second concurrent one.
            if matches!(report.terminal, RunTerminal::Finished) {
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
