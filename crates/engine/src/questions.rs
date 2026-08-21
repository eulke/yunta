//! `kind: questions` validation.
//!
//! Mirrors `findings.rs`'s shape: collect every violation, never just the
//! first. `id` uniqueness and non-empty `text` are cross-entry/basic
//! checks a raw `String` type can't express on its own; `choice` needing
//! non-empty `values` is the one additional rule beyond what the schema
//! itself enforces.

use std::collections::HashSet;

use thiserror::Error;
use yunta_core::{AnswerType, QuestionsFile};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QuestionsError {
    #[error("{id}: duplicate question id")]
    DuplicateId { id: String },

    #[error("{id}: `text` is empty")]
    EmptyText { id: String },

    #[error("{id}: answer_type is `choice` but `values` is empty")]
    MissingValues { id: String },
}

/// Validates a parsed questions file, collecting every violation rather
/// than stopping at the first.
pub fn register(file: &QuestionsFile) -> Vec<QuestionsError> {
    let mut errors = Vec::new();
    let mut known_ids: HashSet<&str> = HashSet::new();

    for question in &file.questions {
        if !known_ids.insert(question.id.as_str()) {
            errors.push(QuestionsError::DuplicateId {
                id: question.id.clone(),
            });
        }
        if question.text.trim().is_empty() {
            errors.push(QuestionsError::EmptyText {
                id: question.id.clone(),
            });
        }
        if question.answer_type == AnswerType::Choice && question.values.is_empty() {
            errors.push(QuestionsError::MissingValues {
                id: question.id.clone(),
            });
        }
    }

    errors
}
