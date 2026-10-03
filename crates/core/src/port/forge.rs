//! The port a gate delegates its multi-person substrate to: publish
//! artifacts and open a pull request, then poll its state on each later
//! wake (`resume`, `status`, a scheduled CI job). **Pull, never push** —
//! no webhooks, no daemon, so nothing has to run anywhere for a gate to
//! work.
//!
//! The trait is here and its implementations are not: the engine drives
//! a gate through this port and never learns which forge answered.

use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

use crate::shown::ShownDocument;
use crate::{CommitSha, InvalidId, NodeId, Responder, RunId};

#[derive(Debug, Error)]
pub enum ForgeError {
    /// No answer: the connection, the TLS handshake or the wait for a
    /// response failed. The cause is whatever the implementation's own
    /// transport reported, boxed — this crate names no HTTP client, and
    /// nothing downstream reads the concrete type: the engine chains it
    /// and a surface prints it.
    #[error("forge: no answer while trying to {action}")]
    Transport {
        action: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
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
        source: Box<dyn std::error::Error + Send + Sync>,
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
#[derive(Debug, Clone, PartialEq)]
pub struct PublishRequest {
    pub branch: String,
    pub base_branch: String,
    /// Carried in the PR body as the run marker, so a later `publish`
    /// call for the same gate finds the open PR it already has instead
    /// of opening a duplicate — idempotent across `resume` invocations.
    /// A closed or merged PR is never reused.
    pub run_id: RunId,
    /// What the pull request asks, and what each answer does to the run.
    pub decision: GateDecision,
    /// (path relative to the repo root, raw content).
    pub artifacts: Vec<(String, Vec<u8>)>,
    /// The documents among `artifacts` a person reads drawn — a plan with
    /// the tests that hold its tasks, a spec, a review's findings — each
    /// published beside its bytes.
    pub shown: Vec<ShownDocument>,
}

/// What a gate published to a forge asks, and what each answer a review
/// can give does to the run — the decision as data, drawn by whoever
/// writes the pull request.
#[derive(Debug, Clone, PartialEq)]
pub struct GateDecision {
    /// The gate that asks.
    pub node: NodeId,
    /// What it asks: the author's `message:`, or what the gate needs.
    pub question: String,
    /// Who it is addressed to.
    pub assignee: String,
    /// What runs once the gate passes, in the workflow's order.
    pub then: Vec<NodeId>,
    /// The node a request for changes sends the run back to, with the
    /// review's comments as its findings; `None` when it fails the gate.
    pub corrected_by: Option<NodeId>,
}

/// A pull request's handle — a published gate's, or the one a
/// `pull_request` node opened — small and forge-opaque on purpose, so it
/// round-trips (as JSON, via `Serialize`/`Deserialize`) through the log
/// without the engine needing to know anything forge-specific about its
/// shape.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PullRequestRef {
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

/// What [`Forge::open_pull_request`] needs: the branch a run pushed, the
/// branch it goes into, and what the pull request says.
pub struct PullRequestRequest {
    pub head: String,
    pub base: String,
    pub title: String,
    pub body: String,
    /// Written into the body as the run's marker, so a later call for the
    /// same run and branch finds the pull request it opened instead of
    /// opening a second one — idempotent across a node's reruns.
    pub run_id: String,
    /// The run's receipt as the pull request opens, drawn into its body
    /// by whoever writes it.
    pub receipt: Option<crate::receipt::Receipt>,
}

/// What a forge says about the credentials a run would reach it with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeProbe {
    /// Whether they may push to the repository; `None` when the forge
    /// does not say.
    pub can_push: Option<bool>,
}

#[async_trait]
pub trait Forge: Send + Sync {
    async fn publish(&self, req: &PublishRequest) -> Result<PullRequestRef, ForgeError>;
    async fn poll(&self, gate: &PullRequestRef) -> Result<PolledGate, ForgeError>;
    /// Opens a pull request of `head`, already pushed, into `base` — or
    /// answers with the open one this run's marker names on that branch.
    /// A closed or merged pull request is never reused.
    async fn open_pull_request(
        &self,
        req: &PullRequestRequest,
    ) -> Result<PullRequestRef, ForgeError>;
    /// Whether the repository answers with these credentials, and what
    /// they may do there.
    async fn probe(&self) -> Result<ForgeProbe, ForgeError>;
}
