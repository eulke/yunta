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
use yunta_core::{AdapterId, ConfigLayer, RunnerCandidate};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunnerError {
    #[error("no `runners:` entry defines role `{role}` — add it to the merged config")]
    UnknownRole { role: String },

    #[error(
        "role `{role}` has no available candidate — tried adapter(s): {}",
        tried.join(", ")
    )]
    NoCandidateAvailable { role: String, tried: Vec<String> },

    #[error(
        "role `{role}` has no candidate on `--adapter {adapter}` — its candidates are on: {}; \
         add one on `{adapter}` to `runners.{role}`, or drop the flag",
        candidates.join(", ")
    )]
    OverrideHasNoCandidate {
        role: String,
        adapter: String,
        candidates: Vec<String>,
    },
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
/// decide what "available" means without this logic changing. With an
/// `adapter_override` (`yunta run --adapter <id>`), only candidates on
/// that adapter qualify; every other candidate is discarded with the
/// override as its reason, so the log says why declaration order did
/// not decide.
pub fn resolve_runner(
    role: &str,
    config: &ConfigLayer,
    available: &dyn Fn(&str) -> bool,
    adapter_override: Option<&AdapterId>,
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
        if let Some(wanted) = adapter_override {
            if candidate.adapter != wanted.as_str() {
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

    let adapters: Vec<String> = candidates.iter().map(|c| c.adapter.clone()).collect();
    match adapter_override {
        Some(wanted) if !adapters.iter().any(|adapter| adapter == wanted.as_str()) => {
            Err(RunnerError::OverrideHasNoCandidate {
                role: role.to_string(),
                adapter: wanted.to_string(),
                candidates: adapters,
            })
        }
        _ => Err(RunnerError::NoCandidateAvailable {
            role: role.to_string(),
            tried: adapters,
        }),
    }
}
