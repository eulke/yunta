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
/// creates or changes, and what it leaves out. What a person approves is
/// what the engine executes.
// Nothing about the brief, the mode or the run: that context lives in the
// manifest and the log. One document for both readers.
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
    /// How the parts the plan touches fit together, in Markdown — the
    /// prose around its `shapes`, which declare the shapes themselves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<String>,
    /// The choices a person could make differently: every point the
    /// brief left open, closed here rather than by whoever implements it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<Decision>,
    /// The shapes the plan creates or changes — types, interfaces,
    /// schemas, signatures, file formats — each declared once, in the
    /// file it lives in, by the one task that builds it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shapes: Vec<Shape>,
    /// What could go wrong that a person approving the plan should weigh.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub risks: Vec<String>,
    /// What the plan deliberately leaves out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub out_of_scope: Vec<String>,
    /// The work, one task per independently verifiable unit.
    pub tasks: Vec<Task>,
}

/// A choice the plan makes that a person could make differently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    /// How a person, and the plan's tasks, refer to it.
    pub id: String,
    /// What was open.
    pub question: String,
    /// What the plan chose.
    pub choice: String,
    /// What it did not choose, one to a line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<String>,
    /// Why this, and not those.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// The question of the run this decision restates, when a person
    /// already answered it: the plan carries their answer and does not
    /// decide it again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answers: Option<crate::QuestionId>,
}

/// A shape the plan creates or changes, declared once where it lives and
/// owned by the task that builds it: every other task that builds on it
/// waits for that one, and none but it may change the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    /// What the plan's tasks call it.
    pub name: String,
    /// The task that builds it.
    pub owner: TaskId,
    /// The file it lives in, which its owner's scope covers.
    pub file: String,
    /// The shape itself, as the code it is.
    pub code: String,
}

/// One place a task changes, and what changes there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Change {
    /// A file, or a file and what in it: `src/theme.rs::Theme`.
    pub at: String,
    /// What changes there, in words.
    pub what: String,
    /// The change as it will read once made: the signature it changes or
    /// the lines it adds — so a person reviewing the plan sees how the
    /// work will look, not only what it is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl Change {
    /// The file the change is in.
    pub fn file(&self) -> &str {
        self.at.split("::").next().unwrap_or(&self.at)
    }
}

impl TasksFile {
    /// A document of `tasks` alone, with nothing for a person to read
    /// beside them.
    pub fn of(tasks: Vec<Task>) -> Self {
        TasksFile {
            summary: None,
            description: None,
            design: None,
            decisions: Vec::new(),
            shapes: Vec::new(),
            risks: Vec::new(),
            out_of_scope: Vec::new(),
            tasks,
        }
    }

    /// What a person reviewing the plan needs that the document does not
    /// say: a summary, a description, why each decision chose what it
    /// did, and for every task its own description, what it changes and
    /// what a person sees once it is done, and for every criterion what
    /// it proves.
    pub fn unexplained(&self) -> Vec<crate::diagnostic::Diagnostic> {
        rules::reviewed(self)
    }
}

/// One criterion: a command, and whether it is a `guard` (passes before
/// and after the work) or an ordinary criterion (fails before the work,
/// passes after).
// The event log freezes it as [`events::Criterion`], which reads what a
// later writer adds; this type refuses it, because an agent wrote it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    /// The command, run under `sh` in the task's checkout; it passes when
    /// it exits 0.
    pub cmd: String,
    /// `guard` for a command that passes before the work and must keep
    /// passing; left out for one that fails before the work and passes
    /// once it is done.
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

/// One task: a unit of work its criteria verify on their own, the paths
/// it may change, and what a person reviewing the plan reads about it.
// `id` is a `TaskId`, so an ill-formed one is a problem of reading the
// document: the report names the path that carries it (`tasks[0].id`) and
// what an id is, never a raw serde error. The rules that only hold across
// the whole document, uniqueness among them, run once it parses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Task {
    /// How the plan's shapes and other tasks refer to it.
    pub id: TaskId,
    /// What the task does, in one line.
    pub title: String,
    /// The only paths the task may change.
    pub scope: Vec<ScopeGlob>,
    /// The commands that must all pass for the task to be done.
    pub criteria: Vec<Criterion>,
    /// The tasks whose work this one starts from. Tasks with no
    /// dependency between them run at the same time and may not share a
    /// path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<TaskId>,
    /// What the session building the task should know that nothing else
    /// here says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// What the task does and why, in Markdown, for a person reviewing
    /// the plan. It names the plan's shapes it touches rather than
    /// repeating them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What the task changes, place by place — each inside its scope.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<Change>,
    /// What a person will observe once the task is done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// The plan's shapes, by name, that the task builds on without
    /// owning them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<String>,
    /// What the code the task touches already promises, which the task
    /// keeps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<String>,
}

mod order;
mod owned;
mod proof;
mod rules;
mod specified;

pub use proof::{names_file, passes_by_a_name};

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

    const SPECIFIED_RULES: &'static [crate::diagnostic::Rule] = specified::SPECIFIED_RULES;
}
