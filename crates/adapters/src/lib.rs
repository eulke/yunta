//! The `Adapter`/`AgentSession` traits and their implementations
//! (`claude-code`, `codex`, `mock`) — Spec Adapter v0.2.
//!
//! `claude-code` and `codex` are built. `mock` is what CI exercises the
//! engine against — the two real adapters are each tested against their
//! own scripted fake binary, never a real LLM in CI.

// A panic is a bug, never a fallible path: production returns a typed
// error instead of unwrapping, expecting, or panicking. Tests are the one
// place a failed assertion is meant to abort — `clippy.toml` lifts these
// there, and integration tests are separate crates this attribute never
// reaches.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

mod claude_code;
mod codex;
pub mod failure;
mod forge;
mod mock;
pub mod process_start;
mod session;
pub mod signal;
pub mod subprocess;

pub use claude_code::{ClaudeCodeAdapter, ID as CLAUDE_CODE_ID};
pub use codex::{CodexAdapter, ID as CODEX_ID};
pub use forge::{
    Forge, ForgeError, GitHubForge, MockForge, MockForgeState, PolledGate, PublishRequest,
    PublishedGate, ReviewComment, ReviewOutcome,
};
pub use mock::{
    MockAdapter, MockEffect, MockFixture, MockOutcome, MockStep, OnInterrupt, SessionScript,
    ID as MOCK_ID,
};
pub use session::{
    Adapter, AgentError, AgentEvent, AgentOutcome, AgentSession, Budget, Glob, PermissionProfile,
    ProbeReport, RunToolsEndpoint, SessionRequest,
};
