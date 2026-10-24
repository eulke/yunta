//! Runner resolution.
//!
//! A node names a runner; the run's frozen config maps that name to an
//! ordered candidate list, mixing adapters, models and agents freely.
//! Resolution walks the list and picks the first candidate whose adapter
//! this invocation built — on the adapter the run is pinned to, when its
//! manifest pins one — recording every discarded candidate with its reason:
//! `runner_resolved` makes the choice auditable, never implicit. The config
//! is frozen and the pin travels with the run, so a resume, a detached run
//! and a successor resolve the same way.
//!
//! What a node needs of its adapter is not a reason to pass a candidate
//! over here: `check` refuses, before the run, a workflow whose
//! declarations no candidate can honor. Availability is the only fallback.
//! Escalating quality — a cheap model first, a costly one if it fails — is
//! a re-route to a node whose runner costs more, visible in the graph,
//! never a second pick here.

use thiserror::Error;
use yunta_core::events::{DiscardedCandidate, RunnerRejection};
use yunta_core::{AdapterId, ConfigLayer, RunnerName};

/// Why a node's runner resolves to no adapter. The display is a code; the
/// words a person reads are written by the CLI from these fields.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunnerError {
    #[error("unknown_runner")]
    UnknownRunner { runner: RunnerName },

    #[error("no_candidate")]
    NoCandidateAvailable {
        runner: RunnerName,
        tried: Vec<AdapterId>,
    },
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
