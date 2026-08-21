//! One module per subcommand; `main.rs` only parses and dispatches.

pub mod cancel;
pub mod doctor;
pub mod gc;
pub mod init;
pub mod list;
pub mod mcp;
pub mod new;
pub mod pack;
pub(crate) mod promote;
pub mod receipt;
pub mod resolve_gate;
pub mod resume;
pub mod run;
pub mod stats;
pub mod status;
pub mod test;
pub mod verify;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use yunta_adapters::{Adapter, ClaudeCodeAdapter, CodexAdapter, Forge, GitHubForge};
use yunta_core::{ConfigLayer, Workflow};
use yunta_engine::{RunReport, RunTerminal};

/// DI-08: Ctrl-C → the run's root `CancellationToken`. The in-process
/// interrupt→kill path (T3.3/T4.6) does the actual exterminating; this
/// only bridges the signal to the token and tells the user what's
/// happening. Installing the handler means SIGINT no longer kills the
/// process outright — the run pauses cleanly with `run_paused
/// { reason: "cancelled by user" }` instead.
pub(crate) fn cancel_on_ctrl_c() -> tokio_util::sync::CancellationToken {
    let root = tokio_util::sync::CancellationToken::new();
    let token = root.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("interrupt received — stopping the run (sessions get interrupt, then kill)");
            token.cancel();
        }
    });
    root
}

/// M8/D101: hands a run off to a fully independent `yunta resume` and
/// returns without waiting on it — what `run --detach` and
/// `resolve-gate` both need (a control-plane operation that must never
/// block for the run's own duration). No new execution path: the
/// detached child is an ordinary resume, exactly what a human would run
/// by hand. Its own log goes to `run.dir/scratch/detached.log` (never
/// silently discarded); its process group is its own so a signal to
/// *this* invocation's group (a shell's Ctrl-C) can never reach it.
pub(crate) fn spawn_detached_resume(
    run_dir: &Path,
    run_id: &str,
    cwd: &Path,
) -> std::io::Result<()> {
    let log_path = run_dir.join("scratch/detached.log");
    let log = std::fs::File::create(&log_path)?;
    let log_err = log.try_clone()?;
    let mut child_cmd = std::process::Command::new(
        std::env::current_exe().unwrap_or_else(|_| PathBuf::from("yunta")),
    );
    child_cmd
        .arg("resume")
        .arg(run_id)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(log_err);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        child_cmd.process_group(0);
    }
    child_cmd.spawn()?;
    // Deliberately not awaited, not tracked: the whole point is that
    // this run's life stops depending on this process the moment it's
    // launched (I25's own rule, applied here to the CLI launcher rather
    // than an MCP session).
    Ok(())
}

/// Prints a run's outcome and maps it to an exit code: success only when
/// the run finished.
pub(crate) fn report_outcome(run_id: &str, report: &RunReport) -> ExitCode {
    match &report.terminal {
        RunTerminal::Finished => {
            println!("run {run_id}: finished");
            ExitCode::SUCCESS
        }
        RunTerminal::Paused { reason } => {
            println!("run {run_id}: paused — {reason}");
            ExitCode::FAILURE
        }
        // `run`/`resume` always route a fresh `RunReport` through
        // `promote::drive_promotions` first — by the time anything
        // calls `report_outcome`, a `Promoted` terminal has already
        // been chased to whatever it became next.
        RunTerminal::Promoted { suggested_mode } => {
            println!("run {run_id}: promoted to `{suggested_mode}` (unresolved)");
            ExitCode::FAILURE
        }
    }
}

/// The adapter registry a real invocation can offer: `claude-code` (T7.3)
/// and `codex` (T7.4), each built only when `runners:` names it as a
/// candidate somewhere in the merged config, with that adapter's own
/// settings (a `binary` override, if declared). Mock fixtures stay
/// routed through `yunta test` only — real invocations never touch the
/// mock (A8 the other way around: a real run never gets a simulated
/// agent either).
pub(crate) fn real_adapters(config: &ConfigLayer) -> HashMap<String, Arc<dyn Adapter>> {
    let mut adapters: HashMap<String, Arc<dyn Adapter>> = HashMap::new();
    let named: Vec<&str> = config
        .runners
        .iter()
        .flatten()
        .flat_map(|(_, candidates)| candidates.iter())
        .map(|candidate| candidate.adapter.as_str())
        .collect();

    let settings_for = |id: &str| {
        config
            .adapters
            .as_ref()
            .and_then(|adapters| adapters.get(id))
            .cloned()
            .unwrap_or_default()
    };

    if named.contains(&"claude-code") {
        adapters.insert(
            "claude-code".to_string(),
            Arc::new(ClaudeCodeAdapter::new(&settings_for("claude-code"))),
        );
    }
    if named.contains(&"codex") {
        adapters.insert(
            "codex".to_string(),
            Arc::new(CodexAdapter::new(&settings_for("codex"))),
        );
    }

    adapters
}

/// The forge a real invocation can offer (§5.6, D66, T7.7) — `None`
/// when either `forge.github` isn't configured, or (D66's own
/// "degrada a consola") the named `token_env` isn't actually set in
/// *this* process's environment. `yunta check` already refuses a
/// workflow with an external gate when the former is missing; the
/// latter is a legitimate, expected runtime state — person B's machine,
/// with no credentials at all, still runs `yunta` just fine, it only
/// ever falls back to the console for a gate it can't reach the forge
/// for.
pub(crate) fn real_forge(config: &ConfigLayer) -> Option<Arc<dyn Forge>> {
    let github = config.forge.as_ref()?.github.as_ref()?;
    let token = std::env::var(&github.token_env).ok()?;
    Some(Arc::new(GitHubForge::new(github.repo.clone(), token)))
}

/// Refuses early when `workflow` needs agent sessions no available
/// adapter can provide (A6: error in check, not emulation at runtime).
pub(crate) fn refuse_unrunnable(
    workflow: &Workflow,
    adapters: &HashMap<String, Arc<dyn Adapter>>,
) -> Result<(), ExitCode> {
    let needs_sessions = workflow.nodes.iter().any(|node| {
        matches!(
            node.kind,
            yunta_core::NodeKind::Prompt { .. } | yunta_core::NodeKind::Loop { .. }
        )
    });
    if needs_sessions && adapters.is_empty() {
        eprintln!(
            "error: this workflow has prompt/loop nodes but `runners:` in the merged config\n\
             names no adapter this binary can run (only `claude-code`/T7.3 and `codex`/T7.4\n\
             are built). To exercise this workflow with the `mock` adapter instead, declare\n\
             a test case under .yunta/tests/ and run `yunta test`."
        );
        return Err(ExitCode::FAILURE);
    }
    Ok(())
}

/// Health-checks every real adapter this run would use (Spec Adapter §2:
/// `probe()` "corre en `yunta doctor` y al crear runs") — binary
/// present, version compatible, auth valid — and refuses before any
/// worktree or token is spent if one comes back unhealthy. `yunta
/// doctor` (T7.1) calls the same adapters' `probe()` directly instead of
/// through this helper, since it reports every result rather than
/// stopping at the first failure.
pub(crate) async fn probe_or_refuse(
    adapters: &HashMap<String, Arc<dyn Adapter>>,
) -> Result<(), ExitCode> {
    let mut unhealthy = Vec::new();
    for (name, adapter) in adapters {
        match adapter.probe().await {
            Ok(report) if report.healthy => {}
            Ok(report) => unhealthy.push(format!(
                "{name}: {}",
                report.diagnostic.as_deref().unwrap_or("unhealthy")
            )),
            Err(e) => unhealthy.push(format!("{name}: {e}")),
        }
    }
    if unhealthy.is_empty() {
        return Ok(());
    }
    eprintln!("error: adapter health check failed — run `yunta doctor` for detail:");
    for line in &unhealthy {
        eprintln!("  {line}");
    }
    Err(ExitCode::FAILURE)
}

/// `yunta check` before running anything — a workflow that fails static
/// validation never creates a run. `workflow_path` is where `workflow`
/// itself was loaded from — needed to tell `check_workflow_refs`
/// whether this workflow already lives inside a pack (T11.3: the
/// cross-pack composition rule only applies once you're inside one).
pub(crate) fn check_or_refuse(
    workflow: &Workflow,
    config: &ConfigLayer,
    workflow_path: &std::path::Path,
) -> Result<(), ExitCode> {
    // Warnings (D100: a `parallel` group that can't verify its children
    // won't collide) are visible but never block — only `check()`'s
    // errors do.
    for warning in yunta_engine::check_warnings(workflow, config) {
        eprintln!("warning: {warning}");
    }
    let mut errors = yunta_engine::check(workflow, config);
    // T9.3: the composition reference graph (`use:` names resolve,
    // acyclic, within depth) reads the repo catalog under the current
    // directory — the same `.yunta/workflows/` a run's children resolve
    // against at birth.
    if let Ok(cwd) = std::env::current_dir() {
        let origin = yunta_engine::origin_of(&cwd, workflow_path);
        errors.extend(yunta_engine::check_workflow_refs(
            workflow, config, &cwd, &origin,
        ));
    }
    if errors.is_empty() {
        return Ok(());
    }
    eprintln!(
        "error: the workflow fails `yunta check` with {} error(s):",
        errors.len()
    );
    for error in &errors {
        eprintln!("  {error}");
    }
    Err(ExitCode::FAILURE)
}
