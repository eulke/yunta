#![forbid(unsafe_code)]

//! The `Adapter`/`AgentSession` traits and their implementations
//! (`claude-code`, `codex`, `mock`) — Spec Adapter v0.2.
//!
//! `claude-code` (T7.3) and `codex` (out of M-0 scope) aren't built yet;
//! `mock` (T3.2) is, and is what CI exercises the engine against (A8).

mod mock;
mod session;

pub use mock::{MockAdapter, MockEffect, MockFixture, MockOutcome, MockStep};
pub use session::{
    Adapter, AgentError, AgentEvent, AgentOutcome, AgentSession, Budget, Glob, PermissionProfile,
    ProbeReport, SessionRequest,
};

/// Identifies this crate to integration tests elsewhere in the workspace.
pub const CRATE_NAME: &str = "yunta-adapters";

/// Name of the crate this one depends on, used by T0.1's integration test
/// to prove the `adapters → core` edge is wired and not just declared.
pub fn depends_on() -> &'static str {
    yunta_core::CRATE_NAME
}
