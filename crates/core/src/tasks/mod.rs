//! The tasks schema: the types a tasks document parses into, the shape
//! published to whoever writes one, and the rules that only hold across
//! the whole document.
//!
//! All three live together because they are one schema. Split across
//! crates, a caller could reach the types without the rules, and one
//! did.

use serde::{Deserialize, Serialize};

use crate::events::{self, CriterionType};
use crate::glob::ScopeGlob;
use crate::TaskId;

/// A tasks document: the tasks the engine runs, and what a person who
/// reviews the plan reads about it — what changes and why, the shapes it
/// creates or changes, and what it leaves out. Nothing about the brief,
/// the mode or the run: that context lives in the manifest and the log.
///
/// One document for both readers, so what a person approves is what the
/// engine executes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TasksFile {
    /// What the plan changes, in one line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// What changes, why, and how the work is approached, in Markdown:
    /// code blocks for an example or how the parts interact, `mermaid`
    /// blocks for diagrams.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The shapes the plan creates or changes — types, interfaces,
    /// schemas, signatures, file formats — as Markdown code blocks, each
    /// declared once for every task that touches it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub risks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub out_of_scope: Vec<String>,
    pub tasks: Vec<Task>,
}

impl TasksFile {
    /// A document of `tasks` alone, with nothing for a person to read
    /// beside them.
    pub fn of(tasks: Vec<Task>) -> Self {
        TasksFile {
            summary: None,
            description: None,
            design: None,
            risks: Vec::new(),
            out_of_scope: Vec::new(),
            tasks,
        }
    }

    /// What a person reviewing the plan needs that the document does not
    /// say: a summary, a description, and for every task its own and for
    /// every criterion what it proves.
    pub fn unexplained(&self) -> Vec<crate::diagnostic::Diagnostic> {
        rules::reviewed(self)
    }
}

/// One criterion as the tasks document declares it: a command, and whether it
/// is a `guard` (passes before and after the work) or an ordinary
/// criterion (red before, green after). The event log freezes it as
/// [`events::Criterion`], which reads what a later writer adds; this
/// type refuses it, because an agent wrote it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub cmd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<CriterionType>,
    /// What passing shows, in words a person reviewing the plan reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proves: Option<String>,
}

impl Criterion {
    /// Whether the criterion is a no-regression guard.
    pub fn is_guard(&self) -> bool {
        self.r#type == Some(CriterionType::Guard)
    }
}

impl From<&Criterion> for events::Criterion {
    fn from(criterion: &Criterion) -> Self {
        events::Criterion {
            cmd: criterion.cmd.clone(),
            r#type: criterion.r#type.clone(),
        }
    }
}

/// One task. `id` is a `TaskId`, so an ill-formed one is a problem of
/// reading the document: the report names the path that carries it
/// (`tasks[0].id`) and what an id is, never a raw serde error. The rules
/// that only hold across the whole document, uniqueness among them, run
/// once it parses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub scope: Vec<ScopeGlob>,
    pub criteria: Vec<Criterion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// What the task does and why, in Markdown, for a person reviewing
    /// the plan. It names the shapes of the plan's `design` it touches
    /// rather than repeating them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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

impl crate::shape::Document for TasksFile {
    const KIND: crate::ArtifactKind = crate::ArtifactKind::Tasks;
    const EXAMPLE: &'static str = EXAMPLE;

    fn check(&self) -> Vec<crate::diagnostic::Diagnostic> {
        rules::check(self)
    }

    const RULES: &'static [crate::diagnostic::Rule] = rules::RULES;

    const RUN_RULES: &'static [crate::diagnostic::Rule] = rules::RUN_RULES;

    const REVIEW_RULES: &'static [crate::diagnostic::Rule] = rules::REVIEW_RULES;
}
