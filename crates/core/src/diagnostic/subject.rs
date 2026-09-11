//! Who is at fault, named the way the document names it.
//!
//! Every entry carries both its id and its position. The id is how a
//! reader knows which one it is; the position is the recourse for the
//! case that makes a deserializer's path useless, which is when the id
//! itself is what failed to parse.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::TaskId;
use crate::{FindingId, QuestionId};

/// Who is at fault, in the vocabulary of the document.
///
/// Every entry carries both its id and its position: the id is how a
/// reader knows which one it is, and the position is the fallback for
/// the case that makes a serde path useless — the id itself is the
/// thing that failed to parse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "of", rename_all = "kebab-case")]
pub enum Subject {
    /// The file as a whole.
    Document,
    Task {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<TaskId>,
        index: usize,
    },
    Criterion {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task: Option<TaskId>,
        /// The task's own position, so a criterion still says which task
        /// it belongs to when that task's id is the thing that could not
        /// be read — the case where naming it by id is impossible and
        /// naming it not at all leaves a reader with nowhere to look.
        #[serde(default)]
        task_index: usize,
        index: usize,
    },
    Finding {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<FindingId>,
        index: usize,
    },
    Question {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<QuestionId>,
        index: usize,
    },
}

impl Subject {
    /// The noun an error uses for this kind of entry, so a message can
    /// say "a task declares ..." without the caller knowing which
    /// subject it holds.
    pub fn noun(&self) -> &'static str {
        match self {
            Subject::Document => "document",
            Subject::Task { .. } => "task",
            Subject::Criterion { .. } => "criterion",
            Subject::Finding { .. } => "finding",
            Subject::Question { .. } => "question",
        }
    }
}

/// `the first task`, `the 5th finding` — how an entry is named when its
/// own id could not be read.
fn ordinal(noun: &str, index: usize) -> String {
    let position = index + 1;
    let word = match position {
        1 => "first".to_string(),
        2 => "second".to_string(),
        3 => "third".to_string(),
        n => format!("{n}{}", ordinal_suffix(n)),
    };
    format!("the {word} {noun}")
}

fn ordinal_suffix(n: usize) -> &'static str {
    match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Subject::Document => f.write_str("the document"),
            Subject::Task { id: Some(id), .. } => write!(f, "task `{id}`"),
            Subject::Task { id: None, index } => f.write_str(&ordinal("task", *index)),
            Subject::Criterion {
                task: Some(task),
                index,
                ..
            } => write!(f, "task `{task}`, criterion {}", index + 1),
            Subject::Criterion {
                task: None,
                task_index,
                index,
            } => write!(
                f,
                "{}, criterion {}",
                ordinal("task", *task_index),
                index + 1
            ),
            Subject::Finding { id: Some(id), .. } => write!(f, "finding `{id}`"),
            Subject::Finding { id: None, index } => f.write_str(&ordinal("finding", *index)),
            Subject::Question { id: Some(id), .. } => write!(f, "question `{id}`"),
            Subject::Question { id: None, index } => f.write_str(&ordinal("question", *index)),
        }
    }
}
