//! `MockForge`: a fixture-driven `Forge` double the engine's own tests
//! exercise the full gate state machine against — publish, poll,
//! approve, request changes, close, and push a commit after approval —
//! all in-memory, no network. Shared via
//! [`MockForgeState`] so a test can hold one handle to drive "what
//! person B does on the forge" while a completely separate
//! `execute_run` call (simulating "person A's machine, a later
//! `resume`") polls the same state through the ordinary `Forge` trait.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use super::{
    Forge, ForgeError, PolledGate, PublishRequest, PublishedGate, ReviewComment, ReviewOutcome,
};

#[derive(Debug, Clone, PartialEq)]
enum Review {
    Pending,
    Approved {
        by: String,
        reviewed_sha: String,
    },
    ChangesRequested {
        by: String,
        reviewed_sha: String,
        comments: Vec<ReviewComment>,
    },
    Closed,
}

struct MockPr {
    number: u64,
    url: String,
    head_sha: String,
    review: Review,
}

#[derive(Default)]
struct Inner {
    /// Keyed by `run_id` — mirrors how the real `GitHubForge` finds an
    /// existing PR for a gate it's already published: idempotent
    /// across `resume`.
    prs: HashMap<String, MockPr>,
    next_number: u64,
    next_sha: u64,
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

    fn fresh_sha(inner: &mut Inner) -> String {
        inner.next_sha += 1;
        format!("sha{:040}", inner.next_sha)
    }

    /// Person B approves — no Yunta involved on their end, just this
    /// call standing in for "clicked Approve in the forge's own UI".
    /// The approval covers whatever the PR's head is *right now*.
    pub fn approve(&self, run_id: &str, by: &str) {
        let mut inner = self.0.lock().unwrap();
        let head = inner
            .prs
            .get(run_id)
            .map(|pr| pr.head_sha.clone())
            .unwrap_or_default();
        if let Some(pr) = inner.prs.get_mut(run_id) {
            pr.review = Review::Approved {
                by: by.to_string(),
                reviewed_sha: head,
            };
        }
    }

    pub fn request_changes(&self, run_id: &str, by: &str, comments: Vec<ReviewComment>) {
        let mut inner = self.0.lock().unwrap();
        let head = inner
            .prs
            .get(run_id)
            .map(|pr| pr.head_sha.clone())
            .unwrap_or_default();
        if let Some(pr) = inner.prs.get_mut(run_id) {
            pr.review = Review::ChangesRequested {
                by: by.to_string(),
                reviewed_sha: head,
                comments,
            };
        }
    }

    pub fn close(&self, run_id: &str) {
        let mut inner = self.0.lock().unwrap();
        if let Some(pr) = inner.prs.get_mut(run_id) {
            pr.review = Review::Closed;
        }
    }

    /// A new commit lands on the PR after it was already approved —
    /// deliberately leaves `review` untouched: real forges don't clear a
    /// submitted review just because the branch moved. Its `reviewed_sha`
    /// now differs from the new `head_sha`; the engine (not this mock,
    /// not `GitHubForge`) is what compares the two and decides the
    /// approval no longer covers the current code, detecting the drift
    /// by SHA.
    pub fn push_commit(&self, run_id: &str) -> String {
        let mut inner = self.0.lock().unwrap();
        let sha = Self::fresh_sha(&mut inner);
        if let Some(pr) = inner.prs.get_mut(run_id) {
            pr.head_sha = sha.clone();
        }
        sha
    }

    pub fn pr_number(&self, run_id: &str) -> Option<u64> {
        self.0.lock().unwrap().prs.get(run_id).map(|pr| pr.number)
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
        if let Some(existing) = inner.prs.get(&req.run_id) {
            return Ok(PublishedGate {
                url: existing.url.clone(),
                number: existing.number,
            });
        }
        inner.next_number += 1;
        let number = inner.next_number;
        let url = format!("https://mock.forge/pr/{number}");
        let head_sha = MockForgeState::fresh_sha(&mut inner);
        inner.prs.insert(
            req.run_id.clone(),
            MockPr {
                number,
                url: url.clone(),
                head_sha,
                review: Review::Pending,
            },
        );
        Ok(PublishedGate { url, number })
    }

    async fn poll(&self, gate: &PublishedGate) -> Result<PolledGate, ForgeError> {
        let inner = self.state.0.lock().unwrap();
        let pr = inner
            .prs
            .values()
            .find(|pr| pr.number == gate.number)
            .ok_or_else(|| ForgeError::UnexpectedResponse(format!("no PR #{}", gate.number)))?;
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
        };
        Ok(PolledGate {
            head_sha: pr.head_sha.clone(),
            review,
        })
    }
}
