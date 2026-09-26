//! The two forges this workspace builds, behind `yunta_core::port::Forge`.
//!
//! `GitHubForge` talks to GitHub's REST API directly — no local git
//! needed, since the Contents and Refs endpoints create a branch and
//! commit files over HTTP alone. `MockForge` is the fixture-driven
//! double the engine's own gate tests run against: the same "never a
//! real network call in CI" stance taken for LLM adapters, extended to
//! forges.

mod github;
mod mock;

pub use github::GitHubForge;
pub use mock::{MockForge, MockForgeState};
