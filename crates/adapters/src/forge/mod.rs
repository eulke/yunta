//! The `Forge` trait — the multi-person substrate an external gate
//! delegates to: publish artifacts + open a PR, then poll its state on
//! each later wake (`resume`, `status`, a scheduled CI job). **Pull,
//! never push** — no webhooks, no daemon, so no infrastructure needs to
//! run anywhere for a gate to work.
//!
//! `GitHubForge` (v1's only real implementation) talks to GitHub's REST
//! API directly — no local git needed, since GitHub's own Contents/Refs
//! endpoints can create a branch and commit files over HTTP alone.
//! `MockForge` is the fixture-driven double the engine's own tests
//! exercise the whole gate state machine against — same "never a real
//! network call in CI" stance taken for LLM adapters, extended here to
//! forges.

mod github;
mod mock;

pub use github::GitHubForge;
pub use mock::{MockForge, MockForgeState};

use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;
use yunta_core::{CommitSha, InvalidId, Responder};

#[derive(Debug, Error)]
pub enum ForgeError {
    /// No answer: the connection, the TLS handshake or the wait for a
    /// response failed.
    #[error("forge: no answer while trying to {action}")]
    Transport {
        action: &'static str,
        #[source]
        source: reqwest::Error,
    },
    /// The forge answered with a refusal, kept as it came.
    #[error("forge: failed to {action}: HTTP {status}: {body}")]
    Http {
        action: &'static str,
        status: u16,
        body: String,
    },
    /// The forge refused for now: the rate limit is spent. `retry_after`
    /// is the wait it named, when it named one.
    #[error(
        "forge: rate limited while trying to {action}{}",
        retry_after.map(|wait| format!(" — retry in {}s", wait.as_secs())).unwrap_or_default()
    )]
    RateLimited {
        action: &'static str,
        retry_after: Option<Duration>,
    },
    /// The forge answered success with a body that is not the shape the
    /// call expects.
    #[error("forge: the answer to {action} is not the shape expected")]
    Response {
        action: &'static str,
        #[source]
        source: reqwest::Error,
    },
    /// The forge answered with a value that is not what it stands for —
    /// a commit id that is not hex, for instance.
    #[error("forge: the answer to {action}: {source}")]
    Malformed {
        action: &'static str,
        #[source]
        source: InvalidId,
    },
    /// A gate handle the forge does not know.
    #[error("forge: no gate #{number}")]
    UnknownGate { number: u64 },
}

/// What `publish` needs to commit and open a PR. The trait itself does
/// no filesystem I/O — `artifacts` are already read off disk by the
/// caller, so a mock forge never needs a real run.dir to be exercised.
pub struct PublishRequest {
    pub branch: String,
    pub base_branch: String,
    /// Carried in the PR body as the run marker, so a later `publish`
    /// call for the same gate finds the open PR it already has instead
    /// of opening a duplicate — idempotent across `resume` invocations.
    /// A closed or merged PR is never reused.
    pub run_id: String,
    pub summary: String,
    /// (path relative to the repo root, raw content).
    pub artifacts: Vec<(String, Vec<u8>)>,
}

/// A published PR's handle — small and forge-opaque on purpose, so it
/// round-trips (as JSON, via `Serialize`/`Deserialize`) through
/// `GateWaitingPayload.external_ref` without the engine needing to know
/// anything forge-specific about its shape.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PublishedGate {
    pub url: String,
    pub number: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReviewComment {
    pub author: String,
    pub body: String,
    pub path: Option<String>,
}

/// A poll's raw facts — deliberately keeps `head_sha` (the PR's current
/// commit) separate from a review's own `reviewed_sha` (the commit it
/// was actually submitted against): comparing the two is what lets the
/// engine (not this trait, and not either implementation) tell a review
/// that still covers the current code from a stale one, uniformly for
/// "resolve this gate for the first time" and "did an approved gate's
/// approval survive a later push" — the engine detects that drift by SHA.
#[derive(Debug, Clone, PartialEq)]
pub struct PolledGate {
    pub head_sha: CommitSha,
    pub review: ReviewOutcome,
}

/// What the forge says about a published gate.
#[derive(Debug, Clone, PartialEq)]
pub enum ReviewOutcome {
    Pending,
    Approved {
        by: Responder,
        reviewed_sha: CommitSha,
    },
    ChangesRequested {
        by: Responder,
        reviewed_sha: CommitSha,
        comments: Vec<ReviewComment>,
    },
    /// Closed without merging.
    Closed,
    /// Merged: approved and landed. `merge_sha` is the merge commit.
    Merged {
        by: Responder,
        merge_sha: CommitSha,
    },
}

#[async_trait]
pub trait Forge: Send + Sync {
    async fn publish(&self, req: &PublishRequest) -> Result<PublishedGate, ForgeError>;
    async fn poll(&self, gate: &PublishedGate) -> Result<PolledGate, ForgeError>;
}
