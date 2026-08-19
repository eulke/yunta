//! The `kind: questions` artifact schema (§4.1, T5.14) — parsed once at
//! the frontier into these types; validation against the field rules is
//! T5.14, in `yunta-engine` (mirrors `ledger.rs`'s own split: types here,
//! `register()` in the engine).

use serde::{Deserialize, Serialize};

/// `text | choice | boolean` (§4.1, verbatim).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerType {
    Text,
    Choice,
    Boolean,
}

/// One question (§4.1): `id`, `text`, `answer_type`, `values` only when
/// `answer_type` is `choice`, and `required`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub text: String,
    pub answer_type: AnswerType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    pub required: bool,
}

/// A `kind: questions` artifact's document — sole top-level key
/// `questions:`, mirroring `Ledger`'s `tasks:`-only shape and
/// `FindingsFile`'s `findings:`-only shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionsFile {
    pub questions: Vec<Question>,
}
