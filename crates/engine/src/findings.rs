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
