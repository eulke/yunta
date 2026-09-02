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
use yunta_core::events::DiscardedCandidate;
use yunta_core::{AdapterId, ConfigLayer, RunnerCandidate, RunnerName};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunnerError {
    #[error("no `runners:` entry defines runner `{runner}` — add it to the merged config")]
    UnknownRunner { runner: RunnerName },

    #[error(
        "runner `{runner}` has no available candidate — tried adapter(s): {}",
        list(tried)
    )]
    NoCandidateAvailable {
        runner: RunnerName,
        tried: Vec<AdapterId>,
    },

    #[error(
        "runner `{runner}` has no candidate on `--adapter {adapter}` — its candidates are on: \
         {}; add one on `{adapter}` to `runners.{runner}`, or drop the flag",
        list(candidates)
    )]
    OverrideHasNoCandidate {
        runner: RunnerName,
        adapter: AdapterId,
        candidates: Vec<AdapterId>,
    },
}

fn list(adapters: &[AdapterId]) -> String {
    adapters
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The outcome of resolving one runner: the winning candidate plus every
/// candidate passed over, with reasons — exactly what `runner_resolved`
/// records.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRunner {
    pub runner: RunnerName,
    pub chosen: RunnerCandidate,
    pub discarded: Vec<DiscardedCandidate>,
}

/// Picks the first candidate of `runner` whose adapter `available`
/// accepts. Pure: availability is injected, so tests and the CLI shell
/// decide what "available" means without this logic changing. With an
/// `adapter_override` (`yunta run --adapter <id>`), only candidates on
/// that adapter qualify; every other candidate is discarded with the
/// override as its reason, so the log says why declaration order did
/// not decide.
pub fn resolve_runner(
    runner: &RunnerName,
    config: &ConfigLayer,
    available: &dyn Fn(&AdapterId) -> bool,
    adapter_override: Option<&AdapterId>,
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
        if let Some(wanted) = adapter_override {
            if candidate.adapter != *wanted {
                discarded.push(DiscardedCandidate {
                    candidate: candidate.clone(),
                    reason: format!(
                        "adapter `{}` is not the `--adapter` override (`{wanted}`)",
                        candidate.adapter
                    ),
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
            reason: format!("adapter `{}` is not available here", candidate.adapter),
        });
    }

    let adapters: Vec<AdapterId> = candidates.iter().map(|c| c.adapter.clone()).collect();
    match adapter_override {
        Some(wanted) if !adapters.contains(wanted) => Err(RunnerError::OverrideHasNoCandidate {
            runner: runner.clone(),
            adapter: wanted.clone(),
            candidates: adapters,
        }),
        _ => Err(RunnerError::NoCandidateAvailable {
            runner: runner.clone(),
            tried: adapters,
        }),
    }
}
