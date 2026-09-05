//! `MockForge`: a fixture-driven `Forge` double the engine's own tests
//! exercise the full gate state machine against — publish, poll,
//! approve, request changes, close, merge, and push a commit after
//! approval — all in-memory, no network, under the same rules as the
//! GitHub forge: a gate keeps the open PR that carries its run marker,
//! and a closed or merged PR is never reused. Shared via
//! [`MockForgeState`] so a test can hold one handle to drive "what
//! person B does on the forge" while a completely separate
//! `execute_run` call (simulating "person A's machine, a later
//! `resume`") polls the same state through the ordinary `Forge` trait.

// A test double whose state lives behind a `Mutex`: the lock is only
// ever poisoned by a test that already panicked while holding it, so a
// double has nothing to recover and unwrapping is the honest response.
#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use yunta_core::CommitSha;

use super::{
    Forge, ForgeError, PolledGate, PublishRequest, PublishedGate, ReviewComment, ReviewOutcome,
};

#[derive(Debug, Clone, PartialEq)]
enum Review {
    Pending,
    Approved {
        by: String,
        reviewed_sha: CommitSha,
    },
    ChangesRequested {
        by: String,
        reviewed_sha: CommitSha,
        comments: Vec<ReviewComment>,
    },
    Closed,
    Merged {
        by: String,
        merge_sha: CommitSha,
    },
}

struct MockPr {
    number: u64,
    run_id: String,
    branch: String,
    url: String,
    head_sha: CommitSha,
    open: bool,
    review: Review,
}

#[derive(Default)]
struct Inner {
    /// Every PR ever published, in order; a run may have more than one
    /// once a person closed an earlier one.
    prs: Vec<MockPr>,
    next_number: u64,
    next_sha: u64,
}

impl Inner {
    /// The run's newest PR — what a person acts on.
    fn latest_for(&mut self, run_id: &str) -> Option<&mut MockPr> {
        self.prs.iter_mut().rev().find(|pr| pr.run_id == run_id)
    }

    /// Mints the next commit id — a real hex id, as git would print one,
    /// so nothing downstream can tell a fixture from a forge.
    fn fresh_sha(&mut self) -> CommitSha {
        self.next_sha += 1;
        CommitSha::from_bytes(&self.next_sha.to_be_bytes())
    }
}

/// The state a test drives directly — everything here is "what happens
/// on the forge", never routed through Yunta: person B never needs
/// Yunta installed.
#[derive(Clone, Default)]
pub struct MockForgeState(Arc<Mutex<Inner>>);

impl MockForgeState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Person B approves — no Yunta involved on their end, just this
    /// call standing in for "clicked Approve in the forge's own UI".
    /// The approval covers whatever the PR's head is *right now*.
    pub fn approve(&self, run_id: &str, by: &str) {
        let mut inner = self.0.lock().unwrap();
        if let Some(pr) = inner.latest_for(run_id) {
            pr.review = Review::Approved {
                by: by.to_string(),
                reviewed_sha: pr.head_sha.clone(),
            };
        }
    }

    pub fn request_changes(&self, run_id: &str, by: &str, comments: Vec<ReviewComment>) {
        let mut inner = self.0.lock().unwrap();
        if let Some(pr) = inner.latest_for(run_id) {
            pr.review = Review::ChangesRequested {
                by: by.to_string(),
                reviewed_sha: pr.head_sha.clone(),
                comments,
            };
        }
    }

    /// Person B closes the PR without merging.
    pub fn close(&self, run_id: &str) {
        let mut inner = self.0.lock().unwrap();
        if let Some(pr) = inner.latest_for(run_id) {
            pr.open = false;
            pr.review = Review::Closed;
        }
    }

    /// Person B merges the PR; returns the merge commit.
    pub fn merge(&self, run_id: &str, by: &str) -> CommitSha {
        let mut inner = self.0.lock().unwrap();
        let merge_sha = inner.fresh_sha();
        if let Some(pr) = inner.latest_for(run_id) {
            pr.open = false;
            pr.review = Review::Merged {
                by: by.to_string(),
                merge_sha: merge_sha.clone(),
            };
        }
        merge_sha
    }

    /// A new commit lands on the PR after it was already approved —
    /// deliberately leaves `review` untouched: real forges don't clear a
    /// submitted review just because the branch moved. Its `reviewed_sha`
    /// now differs from the new `head_sha`; the engine (not this mock,
    /// not `GitHubForge`) is what compares the two and decides the
    /// approval no longer covers the current code, detecting the drift
    /// by SHA.
    pub fn push_commit(&self, run_id: &str) -> CommitSha {
        let mut inner = self.0.lock().unwrap();
        let sha = inner.fresh_sha();
        if let Some(pr) = inner.latest_for(run_id) {
            pr.head_sha = sha.clone();
        }
        sha
    }

    /// The run's newest PR.
    pub fn pr_number(&self, run_id: &str) -> Option<u64> {
        self.0
            .lock()
            .unwrap()
            .latest_for(run_id)
            .map(|pr| pr.number)
    }
}

pub struct MockForge {
    state: MockForgeState,
}

impl MockForge {
    pub fn new(state: MockForgeState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl Forge for MockForge {
    async fn publish(&self, req: &PublishRequest) -> Result<PublishedGate, ForgeError> {
        let mut inner = self.state.0.lock().unwrap();
        let existing = inner
            .prs
            .iter()
            .find(|pr| pr.open && pr.branch == req.branch && pr.run_id == req.run_id);
        if let Some(existing) = existing {
            return Ok(PublishedGate {
                url: existing.url.clone(),
                number: existing.number,
            });
        }
        inner.next_number += 1;
        let number = inner.next_number;
        let url = format!("https://mock.forge/pr/{number}");
        let head_sha = inner.fresh_sha();
        inner.prs.push(MockPr {
            number,
            run_id: req.run_id.clone(),
            branch: req.branch.clone(),
            url: url.clone(),
            head_sha,
            open: true,
            review: Review::Pending,
        });
        Ok(PublishedGate { url, number })
    }

    async fn poll(&self, gate: &PublishedGate) -> Result<PolledGate, ForgeError> {
        let inner = self.state.0.lock().unwrap();
        let pr = inner.prs.iter().find(|pr| pr.number == gate.number).ok_or(
            ForgeError::UnknownGate {
                number: gate.number,
            },
        )?;
        let review = match &pr.review {
            Review::Pending => ReviewOutcome::Pending,
            Review::Approved { by, reviewed_sha } => ReviewOutcome::Approved {
                by: by.clone(),
                reviewed_sha: reviewed_sha.clone(),
            },
            Review::ChangesRequested {
                by,
                reviewed_sha,
                comments,
            } => ReviewOutcome::ChangesRequested {
                by: by.clone(),
                reviewed_sha: reviewed_sha.clone(),
                comments: comments.clone(),
            },
            Review::Closed => ReviewOutcome::Closed,
            Review::Merged { by, merge_sha } => ReviewOutcome::Merged {
                by: by.clone(),
                merge_sha: merge_sha.clone(),
            },
        };
        Ok(PolledGate {
            head_sha: pr.head_sha.clone(),
            review,
        })
    }
}
