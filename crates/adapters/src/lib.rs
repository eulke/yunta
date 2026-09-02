//! The `Adapter`/`AgentSession` traits and their implementations
//! (`claude-code`, `codex`, `mock`) — Spec Adapter v0.2.
//!
//! `claude-code` and `codex` are built. `mock` is what CI exercises the
//! engine against — the two real adapters are each tested against their
//! own scripted fake binary, never a real LLM in CI.

mod claude_code;
mod codex;
mod forge;
mod mock;
pub mod process_start;
mod session;
pub mod signal;

pub use claude_code::{ClaudeCodeAdapter, ID as CLAUDE_CODE_ID};
pub use codex::{CodexAdapter, ID as CODEX_ID};
pub use forge::{
    Forge, ForgeError, GitHubForge, MockForge, MockForgeState, PolledGate, PublishRequest,
    PublishedGate, ReviewComment, ReviewOutcome,
};
pub use mock::{
    MockAdapter, MockEffect, MockFixture, MockOutcome, MockStep, SessionScript, ID as MOCK_ID,
};
pub use session::{
    Adapter, AgentError, AgentEvent, AgentOutcome, AgentSession, Budget, Glob, PermissionProfile,
    ProbeReport, RunToolsEndpoint, SessionRequest,
};
