#![forbid(unsafe_code)]

//! Embedded, append-only event log storage (Contrato §3, D53: SQLite is the
//! only backend — no dialect leaks past this crate's interface).
//!
//! Empty until T2.1; exists now so the workspace dependency graph
//! (core ← storage/adapters ← engine ← cli, T0.1) compiles and is testable.

/// Identifies this crate to integration tests elsewhere in the workspace.
pub const CRATE_NAME: &str = "yunta-storage";

/// Name of the crate this one depends on, used by T0.1's integration test
/// to prove the `storage → core` edge is wired and not just declared.
pub fn depends_on() -> &'static str {
    yunta_core::CRATE_NAME
}
