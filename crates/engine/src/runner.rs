//! Runner resolution.
//!
//! A node names a runner; the merged config's `runners:` maps that name
//! to an ordered candidate list. Resolution walks the list and picks the
//! first candidate whose adapter is actually available, recording every
//! discarded candidate with its reason — `runner_resolved` makes the
//! choice auditable, never implicit. Capability-based discarding (a
//! candidate that lacks a capability the node demands) is a separate,
//! later concern; availability here is simply "is
//! this adapter constructed in this invocation".

use thiserror::Error;
use yunta_core::events::{DiscardedCandidate, RunnerRejection};
use yunta_core::{AdapterId, ConfigLayer, RunnerName};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunnerError {
    #[error("no `runners:` entry defines runner `{runner}` — add it to the merged config")]
    UnknownRunner { runner: RunnerName },

    #[error(
        "runner `{runner}` has no available candidate — tried {}: {}",
        yunta_core::text::agreeing(tried.len(), "adapter", "adapters"),
        list(tried)
    )]
    NoCandidateAvailable {
        runner: RunnerName,
        tried: Vec<AdapterId>,
    },
}

fn list(adapters: &[AdapterId]) -> String {
    adapters
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

pub use yunta_core::events::ResolvedRunner;

/// Picks the first candidate of `runner` whose adapter `available`
/// accepts. Pure: availability is injected, so tests and the CLI shell
/// decide what "available" means without this logic changing. With a
/// `pinned` adapter (the run's own, when it pins one), only candidates on
/// that adapter qualify; every other candidate is discarded as
/// `NotPinned`, so the log says why declaration order did not decide. When
/// no candidate is on the pin, every candidate is discarded and the result
/// is `NoCandidateAvailable`, naming what was tried.
pub fn resolve_runner(
    runner: &RunnerName,
    config: &ConfigLayer,
    available: &dyn Fn(&AdapterId) -> bool,
    pinned: Option<&AdapterId>,
) -> Result<ResolvedRunner, RunnerError> {
    let candidates = config
        .runners
        .as_ref()
        .and_then(|runners| runners.get(runner))
        .ok_or_else(|| RunnerError::UnknownRunner {
            runner: runner.clone(),
        })?;

    let mut discarded = Vec::new();
    for candidate in candidates {
        if let Some(wanted) = pinned {
            if candidate.adapter != *wanted {
                discarded.push(DiscardedCandidate {
                    candidate: candidate.clone(),
                    reason: RunnerRejection::NotPinned {
                        pinned: wanted.clone(),
                    },
                });
                continue;
            }
        }
        if available(&candidate.adapter) {
            return Ok(ResolvedRunner {
                runner: runner.clone(),
                chosen: candidate.clone(),
                discarded,
            });
        }
        discarded.push(DiscardedCandidate {
            candidate: candidate.clone(),
            reason: RunnerRejection::Unavailable,
        });
    }

    let adapters: Vec<AdapterId> = candidates.iter().map(|c| c.adapter.clone()).collect();
    Err(RunnerError::NoCandidateAvailable {
        runner: runner.clone(),
        tried: adapters,
    })
}
