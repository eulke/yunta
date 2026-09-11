//! What a questions artifact is, in the vocabulary its own readers use.

use crate::diagnostic::{Named, Subject};
use crate::shape::{Hint, Walk};
use crate::yaml::Value;
use crate::{AnswerType, QuestionId};

use super::EXAMPLE;

/// The keys a question declares. Restated from [`crate::Question`] and
/// held true by the schema test that reads the generated JSON Schema —
/// see `crate::ledger::shape` for why the restatement exists at all.
pub(crate) const QUESTION_REQUIRED: &[&str] = &["id", "text", "answer_type", "required"];
pub(crate) const QUESTION_OPTIONAL: &[&str] = &["values"];

const QUESTION_HINTS: &[Hint] = &[
    ("question", "text", "the text a person reads is `text:`"),
    ("prompt", "text", "the text a person reads is `text:`"),
    (
        "type",
        "answer_type",
        "the kind of answer is `answer_type:`",
    ),
    (
        "options",
        "values",
        "the answers a `choice` question allows are `values:`",
    ),
    (
        "choices",
        "values",
        "the answers a `choice` question allows are `values:`",
    ),
    (
        "default",
        "",
        "a question has no default: an unanswered one pauses the run",
    ),
];

pub(super) fn diagnose(value: &Value, walk: &mut Walk) {
    walk.entries(value, "questions", EXAMPLE, question);
}

/// One question, named by its own id when that parsed.
fn question(walk: &mut Walk, index: usize, item: &Value) {
    walk.at(Subject::Question(Named::new(None, index)));
    let Some(map) = walk.mapping(item, "a mapping", "- id: theme-source") else {
        return;
    };
    let named = Named::new(walk.id::<QuestionId>(map), index);
    walk.at(Subject::Question(named))
        .keys(map, QUESTION_REQUIRED, QUESTION_OPTIONAL, QUESTION_HINTS)
        .string(map, "text", "text: \"Which theme?\"")
        .string_list(
            map,
            "values",
            "a list of allowed answers",
            "values: [\"teal\", \"amber\"]",
        )
        .one_of(
            map,
            "answer_type",
            &AnswerType::NAMES,
            "answer_type: boolean",
        )
        .boolean(map, "required", "required: true");
}
