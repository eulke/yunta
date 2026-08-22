//! Runner resolution.
//!
//! A node names a role; the merged config's `runners:` maps that role to
//! an ordered candidate list. Resolution walks the list and picks the
//! first candidate whose adapter is actually available, recording every
//! discarded candidate with its reason — `runner_resolved` makes the
//! choice auditable, never implicit. Capability-based discarding (a
//! candidate that lacks a capability the node demands) is a separate,
//! later concern; availability here is simply "is
//! this adapter constructed in this invocation".

use thiserror::Error;
use yunta_core::events::DiscardedCandidate;
use yunta_core::{ConfigLayer, RunnerCandidate};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunnerError {
    #[error("no `runners:` entry defines role `{role}` — add it to the merged config")]
    UnknownRole { role: String },

    #[error(
        "role `{role}` has no available candidate — tried adapter(s): {}",
        tried.join(", ")
    )]
    NoCandidateAvailable { role: String, tried: Vec<String> },
}

/// The outcome of resolving one role: the winning candidate plus every
/// candidate passed over, with reasons — exactly what `runner_resolved`
/// records.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRunner {
    pub role: String,
    pub chosen: RunnerCandidate,
    pub discarded: Vec<DiscardedCandidate>,
}

/// Picks the first candidate of `role` whose adapter `available`
/// accepts. Pure: availability is injected, so tests and the CLI shell
/// decide what "available" means without this logic changing.
pub fn resolve_runner(
    role: &str,
    config: &ConfigLayer,
    available: &dyn Fn(&str) -> bool,
) -> Result<ResolvedRunner, RunnerError> {
    let candidates = config
        .runners
        .as_ref()
        .and_then(|runners| runners.get(role))
        .ok_or_else(|| RunnerError::UnknownRole {
            role: role.to_string(),
        })?;

    let mut discarded = Vec::new();
    for candidate in candidates {
        if available(&candidate.adapter) {
            return Ok(ResolvedRunner {
                role: role.to_string(),
                chosen: candidate.clone(),
                discarded,
            });
        }
        discarded.push(DiscardedCandidate {
            candidate: candidate.clone(),
            reason: format!("adapter `{}` is not available here", candidate.adapter),
        });
    }

    Err(RunnerError::NoCandidateAvailable {
        role: role.to_string(),
        tried: candidates.iter().map(|c| c.adapter.clone()).collect(),
    })
}
