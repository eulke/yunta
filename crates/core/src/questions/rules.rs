//! The rules a questions artifact has to satisfy once it is readable.
//!
//! Shape is the frontier before this one: by the time a
//! [`QuestionsFile`] exists, every key is known and `answer_type` is one
//! of the three. What is left is the rule that spans the document — an
//! id used twice — and the two a type cannot express: text that is
//! blank, and a `choice` with nothing to choose from.

use std::collections::HashSet;

use crate::diagnostic::{Diagnostic, Named, Problem, Rule, RuleCode, Subject};
use crate::{AnswerType, QuestionId, QuestionsFile};

/// Every rule this document is held to — see `crate::ledger::rules` for what
/// this list is for and what holds it true.
pub(super) const RULES: &[Rule] = &[
    Rule {
        code: RuleCode::DuplicateId,
        demand: "each `id` is declared once in the file",
    },
    Rule {
        code: RuleCode::EmptyText,
        demand: "`text` is non-empty: a person has to be able to read the question",
    },
    Rule {
        code: RuleCode::MissingValues,
        demand: "a question whose `answer_type` is `choice` lists the answers it allows in \
                 `values`",
    },
];

fn broke(index: usize, id: &QuestionId, code: RuleCode, detail: &str) -> Diagnostic {
    Diagnostic::new(
        Subject::Question(Named::new(id.clone(), index)),
        Problem::rule(code, detail),
    )
}

/// Every violation the file carries, collected rather than stopped at
/// the first.
pub(super) fn check(file: &QuestionsFile) -> Vec<Diagnostic> {
    let mut broken = Vec::new();
    let mut known_ids: HashSet<&QuestionId> = HashSet::new();

    for (index, question) in file.questions.iter().enumerate() {
        if !known_ids.insert(&question.id) {
            broken.push(broke(
                index,
                &question.id,
                RuleCode::DuplicateId,
                "a second question already carries this id; every id is declared once",
            ));
        }
        if question.text.trim().is_empty() {
            broken.push(broke(
                index,
                &question.id,
                RuleCode::EmptyText,
                "`text` is empty; a person has to be able to read the question",
            ));
        }
        if question.answer_type == AnswerType::Choice && question.values.is_empty() {
            broken.push(broke(
                index,
                &question.id,
                RuleCode::MissingValues,
                "`answer_type` is `choice` but `values` is empty; list the answers allowed",
            ));
        }
    }

    broken
}
