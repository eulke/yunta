//! The adapters that implement `yunta_core::port`: `claude-code`,
//! `codex` and `mock` — Spec Adapter v0.2.
//!
//! `claude-code` and `codex` each drive their own CLI; `mock` is what CI
//! exercises the engine against — the two real adapters are each tested
//! against their own scripted fake binary, never a real LLM in CI.

// A panic is a bug, never a fallible path: production returns a typed
// error instead of unwrapping, expecting, indexing, or panicking.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::indexing_slicing
)]
// Tests are the one place a failed assertion is meant to abort. The panic
// family is lifted there by `clippy.toml`; `indexing_slicing` has no such
// switch, so it is lifted in test builds here. Integration tests are
// separate crates neither reaches.
#![cfg_attr(test, allow(clippy::indexing_slicing))]

mod claude_code;
mod codex;
pub mod failure;
mod forge;
mod mock;
mod session;

pub use claude_code::{ClaudeCodeAdapter, ID as CLAUDE_CODE_ID};
pub use codex::{CodexAdapter, ID as CODEX_ID};
pub use forge::{GitHubForge, MockForge, MockForgeState};
pub use mock::{
    FixtureError, MockAdapter, MockEffect, MockFixture, MockOutcome, MockStep, OnInterrupt,
    RunPaths, SessionScript, ID as MOCK_ID,
};
