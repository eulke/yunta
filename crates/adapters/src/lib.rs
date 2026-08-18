#![forbid(unsafe_code)]

//! The `Adapter`/`AgentSession` traits and their implementations
//! (`claude-code`, `codex`, `mock`) — Spec Adapter v0.2.
//!
//! Empty until T3.1; exists now so the workspace dependency graph
//! (core ← storage/adapters ← engine ← cli, T0.1) compiles and is testable.

/// Identifies this crate to integration tests elsewhere in the workspace.
pub const CRATE_NAME: &str = "yunta-adapters";

/// Name of the crate this one depends on, used by T0.1's integration test
/// to prove the `adapters → core` edge is wired and not just declared.
pub fn depends_on() -> &'static str {
    yunta_core::CRATE_NAME
}
