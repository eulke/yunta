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

/// One entry of a document: by name when its id parsed, by position
/// when the id is the thing that could not be read.
///
/// The pair is one concept, so it is one type. Spelled as two loose
/// fields, the half that is easiest to forget is the position, and a
/// position that goes missing does not read as absent — it reads as
/// zero, and names the first entry with confidence.
// The bound is spelled out because `#[serde(default)]` on a generic
// field otherwise asks for `Id: Default`, and an identifier has no
// default — it is parsed or it is absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(bound(serialize = "Id: Serialize", deserialize = "Id: Deserialize<'de>"))]
pub struct Named<Id> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Id>,
    pub index: usize,
}

impl<Id> Named<Id> {
    pub fn new(id: impl Into<Option<Id>>, index: usize) -> Self {
        Named {
            id: id.into(),
            index,
        }
    }

    /// How this entry names itself, given the noun for its kind.
    fn render(&self, noun: &str) -> String
    where
        Id: fmt::Display,
    {
        match &self.id {
            Some(id) => format!("{noun} `{id}`"),
            None => ordinal(noun, self.index),
        }
    }
}

/// Who is at fault, in the vocabulary of the document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "of", rename_all = "kebab-case")]
pub enum Subject {
    /// The document as a whole, as opposed to an entry inside it.
    Document,
    Task(Named<TaskId>),
    /// A criterion names the task it belongs to, so a reader always has
    /// somewhere to look — including when that task's own id is what
    /// could not be read.
    Criterion {
        task: Named<TaskId>,
        index: usize,
    },
    Finding(Named<FindingId>),
    Question(Named<QuestionId>),
}

impl Subject {}

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
            Subject::Task(task) => f.write_str(&task.render("task")),
            Subject::Criterion { task, index } => {
                write!(f, "{}, criterion {}", task.render("task"), index + 1)
            }
            Subject::Finding(finding) => f.write_str(&finding.render("finding")),
            Subject::Question(question) => f.write_str(&question.render("question")),
        }
    }
}
