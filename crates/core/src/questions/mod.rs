//! The `kind: questions` artifact schema: the types a questions file
//! parses into, the shape published to whoever writes one, and the
//! rules that only hold across the whole document.
//!
//! All three live together because they are one schema.

use serde::{Deserialize, Serialize};

use crate::ids::QuestionId;

/// `text | choice | boolean`, verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnswerType {
    Text,
    Choice,
    Boolean,
}

impl AnswerType {
    /// The answers a question can ask for, as a document writes them.
    /// Tied to what serde derives by a test.
    pub const NAMES: [&'static str; 3] = ["text", "choice", "boolean"];
}

/// One question: `id`, `text`, `answer_type`, `values` only when
/// `answer_type` is `choice`, and `required`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Question {
    pub id: QuestionId,
    pub text: String,
    pub answer_type: AnswerType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    pub required: bool,
}

/// A `kind: questions` artifact's document — sole top-level key
/// `questions:`, mirroring `TasksFile`'s `tasks:`-only shape and
/// `FindingsFile`'s `findings:`-only shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuestionsFile {
    pub questions: Vec<Question>,
}

/// One answer: the question's `id` and the human's value, as
/// text — a `boolean` answer is the string `true`/`false`, a `choice`
/// one of the declared `values`. Strings deliberately, not a typed enum
/// per answer kind: the artifact is what a *following node's session*
/// reads — meant to be consumed by whatever node comes next — and text
/// is the only shape every consumer shares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Answer {
    pub id: QuestionId,
    pub value: String,
}

/// The answers artifact's document — written by the ENGINE, never an
/// agent, next to the questions artifact it answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AnswersFile {
    pub answers: Vec<Answer>,
}

/// Validates a reply against its questions: every `required`
/// question answered, every answer names a declared question, `choice`
/// values within the declared list, `boolean` values parseable. All
/// violations reported together, never just the first (same principle
/// as the tasks document's own registration).
pub fn validate_answers(file: &QuestionsFile, answers: &[Answer]) -> Vec<String> {
    let mut violations = Vec::new();
    for answer in answers {
        let Some(question) = file.questions.iter().find(|q| q.id == answer.id) else {
            violations.push(format!("answer `{}` names no declared question", answer.id));
            continue;
        };
        match question.answer_type {
            AnswerType::Choice => {
                if !question.values.contains(&answer.value) {
                    violations.push(format!(
                        "answer `{}`: `{}` is not one of [{}]",
                        answer.id,
                        answer.value,
                        question.values.join(", ")
                    ));
                }
            }
            AnswerType::Boolean => {
                if answer.value != "true" && answer.value != "false" {
                    violations.push(format!(
                        "answer `{}`: `{}` is not `true`/`false`",
                        answer.id, answer.value
                    ));
                }
            }
            AnswerType::Text => {}
        }
    }
    for question in &file.questions {
        if question.required && !answers.iter().any(|a| a.id == question.id) {
            violations.push(format!("required question `{}` has no answer", question.id));
        }
    }
    violations
}

mod rules;

/// The shape this document publishes, as the YAML it is.
///
/// It lives as a file rather than a string literal, so an editor reads
/// it as YAML and a person reviewing a schema change sees the diff in
/// the format the change is about. `include_str!` binds it at compile
/// time, and the test that reads it back through
/// [`read`](crate::shape::read) is what stops it drifting from the
/// parser.
const EXAMPLE: &str = include_str!("shape.yaml");

impl crate::shape::Document for QuestionsFile {
    const KIND: crate::ArtifactKind = crate::ArtifactKind::Questions;
    const EXAMPLE: &'static str = EXAMPLE;

    fn check(&self) -> Vec<crate::diagnostic::Diagnostic> {
        rules::check(self)
    }

    const RULES: &'static [crate::diagnostic::Rule] = rules::RULES;
}
