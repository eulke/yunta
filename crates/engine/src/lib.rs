#![forbid(unsafe_code)]

//! The workflow engine: DAG scheduler, verification cycle, resumability
//! (Contrato del Run). `yunta-engine` never depends on rusqlite/sqlx
//! directly (D53) and never contains CLI-specific knowledge (A1) — those
//! are enforced by the crate graph itself, not by convention.
//!
//! Empty until T4.x; exists now so the workspace dependency graph
//! (core ← storage/adapters ← engine ← cli, T0.1) compiles and is testable.

mod check;
mod ledger;
mod replay;

pub use check::{check, CheckError};
pub use ledger::{register, LedgerError};
pub use replay::{derive, NodeState, RunState};

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
