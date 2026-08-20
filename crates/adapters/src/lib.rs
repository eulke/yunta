#![forbid(unsafe_code)]

//! The `Adapter`/`AgentSession` traits and their implementations
//! (`claude-code`, `codex`, `mock`) — Spec Adapter v0.2.
//!
//! `claude-code` (T7.3) and `codex` (T7.4) are built. `mock` (T3.2) is
//! what CI exercises the engine against (A8) — the two real adapters
//! are each tested against their own scripted fake binary, never a real
//! LLM in CI.

mod claude_code;
mod codex;
mod mock;
mod session;

pub use claude_code::ClaudeCodeAdapter;
pub use codex::CodexAdapter;
pub use mock::{MockAdapter, MockEffect, MockFixture, MockOutcome, MockStep, SessionScript};
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
