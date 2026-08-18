//! One module per subcommand; `main.rs` only parses and dispatches.

pub mod resume;
pub mod run;
pub mod status;
pub mod test;

use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::Arc;

use yunta_adapters::Adapter;
use yunta_core::{ConfigLayer, Workflow};
use yunta_engine::{RunReport, RunTerminal};

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
    }
}

/// The adapter registry a real invocation can offer today. The
/// `claude-code` adapter is T7.3 (not built yet) and mock fixtures are
/// routed by `yunta test`, so a workflow that needs an agent session
/// cannot run from here yet — that limitation is reported up front,
/// before any run is created, never discovered halfway through one.
pub(crate) fn real_adapters() -> HashMap<String, Arc<dyn Adapter>> {
    HashMap::new()
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
            "error: this workflow has prompt/loop nodes and no agent adapter is available yet.\n\
             The `claude-code` adapter arrives with T7.3. To exercise a workflow with the\n\
             `mock` adapter, declare a test case under .yunta/tests/ and run `yunta test`."
        );
        return Err(ExitCode::FAILURE);
    }
    Ok(())
}

/// `yunta check` before running anything — a workflow that fails static
/// validation never creates a run.
pub(crate) fn check_or_refuse(workflow: &Workflow, config: &ConfigLayer) -> Result<(), ExitCode> {
    let errors = yunta_engine::check(workflow, config);
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
