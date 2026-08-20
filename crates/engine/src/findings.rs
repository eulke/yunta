//! `kind: findings` validation (T5.12, Contrato §4.1).
//!
//! Mirrors `ledger.rs`'s shape: collect every violation, never just the
//! first. What's validated here is structural — the schema itself
//! already forces every field but `proposed_criterion` to be present;
//! this adds the one cross-entry rule (`id` uniqueness) and the
//! non-empty checks a raw `String` type can't express on its own.

use std::collections::HashSet;

use thiserror::Error;
use yunta_core::events::FindingsFile;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FindingsError {
    #[error("{id}: duplicate finding id")]
    DuplicateId { id: String },

    #[error("{id}: `title` is empty")]
    EmptyTitle { id: String },

    #[error("{id}: `location` is empty")]
    EmptyLocation { id: String },

    #[error("{id}: `detail` is empty")]
    EmptyDetail { id: String },
}

/// Validates a parsed findings file, collecting every violation rather
/// than stopping at the first.
pub fn register(file: &FindingsFile) -> Vec<FindingsError> {
    let mut errors = Vec::new();
    let mut known_ids: HashSet<&str> = HashSet::new();

    for finding in &file.findings {
        if !known_ids.insert(finding.id.as_str()) {
            errors.push(FindingsError::DuplicateId {
                id: finding.id.clone(),
            });
        }
        if finding.title.trim().is_empty() {
            errors.push(FindingsError::EmptyTitle {
                id: finding.id.clone(),
            });
        }
        if finding.location.trim().is_empty() {
            errors.push(FindingsError::EmptyLocation {
                id: finding.id.clone(),
            });
        }
        if finding.detail.trim().is_empty() {
            errors.push(FindingsError::EmptyDetail {
                id: finding.id.clone(),
            });
        }
    }

    errors
}

/// §10.2/DI-10: the findings a successor inherits, derived purely from
/// the parent's own log — every `finding_posted`, deduplicated by the
/// normative rule (location + title normalized for case and whitespace).
/// The first occurrence's full record wins, so no authorship or detail
/// is lost to the collapse. Deterministic: same log, same output — the
/// promotion close serializes exactly this into
/// `artifacts/findings-inherited.yaml`.
pub fn inherited_findings(
    events: &[yunta_core::events::Event],
) -> Vec<yunta_core::events::Finding> {
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut inherited = Vec::new();
    for event in events {
        let yunta_core::events::EventPayload::FindingPosted(p) = &event.payload else {
            continue;
        };
        let key = (
            p.finding.location.clone(),
            normalized_title(&p.finding.title),
        );
        if seen.insert(key) {
            inherited.push(p.finding.clone());
        }
    }
    inherited
}

/// Case- and whitespace-insensitive: "Scope  expansion DENIED" and
/// "scope expansion denied" are the same complaint about the same place.
fn normalized_title(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
