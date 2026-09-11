//! The questions artifact's registration rules.
//!
//! Shape is `yunta-core`'s frontier: by the time a `QuestionsFile`
//! exists, every key is known and `answer_type` is one of the three.
//! What is left is the rule that spans the document — an id used twice
//! — and the two a type cannot express: text that is blank, and a
//! `choice` with nothing to choose from.

use std::collections::HashSet;

use yunta_core::diagnostic::{Diagnostic, Problem, Subject};
use yunta_core::{AnswerType, QuestionId, QuestionsFile};

fn broke(index: usize, id: &QuestionId, code: &'static str, detail: &str) -> Diagnostic {
    Diagnostic::new(
        Subject::Question {
            id: Some(id.clone()),
            index,
        },
        Problem::rule(code, detail),
    )
}

/// Validates a parsed questions file, collecting every violation rather
/// than stopping at the first.
pub fn register(file: &QuestionsFile) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    let mut known_ids: HashSet<&QuestionId> = HashSet::new();

    for (index, question) in file.questions.iter().enumerate() {
        if !known_ids.insert(&question.id) {
            errors.push(broke(
                index,
                &question.id,
                "duplicate-id",
                "a second question already carries this id; every id is declared once",
            ));
        }
        if question.text.trim().is_empty() {
            errors.push(broke(
                index,
                &question.id,
                "empty-text",
                "`text` is empty; a person has to be able to read the question",
            ));
        }
        if question.answer_type == AnswerType::Choice && question.values.is_empty() {
            errors.push(broke(
                index,
                &question.id,
                "missing-values",
                "`answer_type` is `choice` but `values` is empty; list the answers allowed",
            ));
        }
    }

    errors
}
