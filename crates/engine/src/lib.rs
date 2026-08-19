#![forbid(unsafe_code)]

//! The workflow engine: DAG scheduler, verification cycle, resumability
//! (Contrato del Run). `yunta-engine` never depends on rusqlite/sqlx
//! directly (D53) and never contains CLI-specific knowledge (A1) — those
//! are enforced by the crate graph itself, not by convention.
//!
//! Empty until T4.x; exists now so the workspace dependency graph
//! (core ← storage/adapters ← engine ← cli, T0.1) compiles and is testable.

mod artifacts;
mod check;
mod findings;
mod ledger;
mod manifest;
mod permissions;
mod progress;
mod replay;
mod run;
mod runner;
mod scope;
mod task_cycle;
mod template;
mod worktree;

pub use artifacts::{close_artifacts, ArtifactError, VerifiedArtifact};
pub use check::{check, check_warnings, CheckError, CheckWarning};
pub use findings::{register as register_findings, FindingsError};
pub use ledger::{register, LedgerError};
pub use manifest::{build_manifest, ManifestError};
pub use permissions::command_violation;
pub use progress::render_progress;
pub use replay::{dedup_findings, derive, NodeState, RunState};
pub use run::{create_run, execute_run, RunError, RunReport, RunTerminal};
pub use runner::{resolve_runner, ResolvedRunner, RunnerError};
pub use scope::{scope_check, ScopeCheckError, ScopeCheckResult};
pub use task_cycle::{
    post_check, pre_check, run_task, AttemptRecord, CriterionRun, DispatchOutcome, Memo,
    PreCheckOutcome, TaskCycleError, TaskCycleReport, TaskOutcome, DEFAULT_MAX_RETRIES,
};
pub use template::{render_template, template_variables, TemplateError};
pub use worktree::{prepare_worktree, release_worktree, WorktreeError};

/// Identifies this crate to integration tests elsewhere in the workspace.
pub const CRATE_NAME: &str = "yunta-engine";

/// The crates this one depends on, in the order T0.1 fixes them, used by
/// the integration test to prove the graph is wired and not just declared.
pub fn depends_on() -> [&'static str; 2] {
    [yunta_storage::CRATE_NAME, yunta_adapters::CRATE_NAME]
}

/// Version string exposed to the CLI, so `crates/cli` has something real
/// to call across the `engine → cli` edge without pre-empting T7.1.
pub fn version_string() -> String {
    format!("yunta-engine {}", env!("CARGO_PKG_VERSION"))
}
