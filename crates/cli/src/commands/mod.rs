//! One module per subcommand; `main.rs` only parses and dispatches.

pub mod resume;
pub mod run;
pub mod status;
pub mod test;

use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::Arc;

use yunta_adapters::{Adapter, ClaudeCodeAdapter};
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

/// The adapter registry a real invocation can offer: `claude-code` (T7.3)
/// when `runners:` names it as a candidate somewhere in the merged
/// config, built with that adapter's settings (a `binary` override, if
/// declared). Mock fixtures stay routed through `yunta test` only — real
/// invocations never touch the mock (A8 the other way around: a real run
/// never gets a simulated agent either).
pub(crate) fn real_adapters(config: &ConfigLayer) -> HashMap<String, Arc<dyn Adapter>> {
    let mut adapters: HashMap<String, Arc<dyn Adapter>> = HashMap::new();

    let names_claude_code = config
        .runners
        .iter()
        .flatten()
        .flat_map(|(_, candidates)| candidates.iter())
        .any(|candidate| candidate.adapter == "claude-code");
    if names_claude_code {
        let settings = config
            .adapters
            .as_ref()
            .and_then(|adapters| adapters.get("claude-code"))
            .cloned()
            .unwrap_or_default();
        adapters.insert(
            "claude-code".to_string(),
            Arc::new(ClaudeCodeAdapter::new(&settings)),
        );
    }

    adapters
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
             names no adapter this binary can run (only `claude-code` is built, T7.3). To\n\
             exercise this workflow with the `mock` adapter instead, declare a test case\n\
             under .yunta/tests/ and run `yunta test`."
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
