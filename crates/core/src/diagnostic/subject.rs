//! Who is at fault, named the way the document names it.
//!
//! Every entry carries both its id and its position. The id is how a
//! reader knows which one it is; the position is the recourse for the
//! case that makes a deserializer's path useless, which is when the id
//! itself is what failed to parse.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::DocumentKind;
use crate::ids::TaskId;
use crate::yaml::Pointer;
use crate::{FindingId, NodeId, QuestionId};

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
    /// One task's spec, named by the task it holds.
    Spec(Named<TaskId>),
    /// A node of a workflow, for a rule about the graph the file
    /// declares.
    Node(Named<NodeId>),
}

/// `the first task`, `the 5th finding` — how an entry is named when its
/// own id could not be read.
fn ordinal(noun: &str, index: usize) -> String {
    let position = index + 1;
    let word = match position {
        1 => "first".to_string(),
        2 => "second".to_string(),
        3 => "third".to_string(),
        n => crate::text::ordinal(n),
    };
    format!("the {word} {noun}")
}

impl Subject {
    /// Where this entry is written in a document of `kind`, from its
    /// root: a task, a criterion, a finding, a question or its answer, a
    /// spec, or a workflow node by its id when the id was read.
    pub fn pointer(&self, kind: DocumentKind) -> Pointer {
        let root = Pointer::root();
        match self {
            Subject::Document => root,
            Subject::Task(task) => root.key("tasks").index(task.index),
            Subject::Criterion { task, index } => root
                .key("tasks")
                .index(task.index)
                .key("criteria")
                .index(*index),
            Subject::Finding(finding) => root.key("findings").index(finding.index),
            Subject::Question(question) => match kind {
                DocumentKind::Artifact(crate::ArtifactKind::Answers) => root.key("answers"),
                _ => root.key("questions"),
            }
            .index(question.index),
            Subject::Spec(spec) => root.key("specs").index(spec.index),
            Subject::Node(node) => match &node.id {
                Some(id) => root.key("nodes").node(id.as_str()),
                None => root.key("nodes").index(node.index),
            },
        }
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
            Subject::Spec(spec) => f.write_str(&spec.render("spec of task")),
            Subject::Node(node) => f.write_str(&node.render("node")),
        }
    }
}
