//! A task's criteria as the log keeps them: what the tasks document
//! declared, and how each one answered when the engine ran it.

use serde::{Deserialize, Serialize};

use crate::hash::{ContentHash, TreeId};

/// A tasks document criterion, frozen into `task_registered` — the same shape
/// the tasks parser produces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Criterion {
    pub cmd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<CriterionType>,
}

/// A criterion's kind: a `guard` is green before the work, and the task has
/// to keep it green.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CriterionType {
    Guard,
}

/// One criterion's outcome inside `criteria_checked.results`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CriterionResult {
    /// The command, as the task declares it.
    pub cmd: String,
    /// The code the command exited with.
    pub exit_code: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<CriterionType>,
    /// Whether the answer is one an earlier run of the same command gave on
    /// the same tree, taken without running the command again.
    pub reused: bool,
    /// Wall-clock milliseconds this execution took — the datum
    /// the learned criterion ordering feeds on. `None` for a `reused: true`
    /// result (nothing ran) and for events logged before this field
    /// existed (additive, tolerant reader).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// The run object holding everything the command printed — stdout,
    /// then stderr — with the run's secrets taken out: for a red answer
    /// `reused`, what the run that gave it printed. Absent when the
    /// command printed nothing, for a green answer `reused`, and on a log
    /// written before a run kept what its commands printed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<ContentHash>,
    /// The last lines a criterion that did not pass printed: what a
    /// reader of the log, a person deciding and the next session read to
    /// know why, without opening the object. Empty for one that passed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tail: Vec<String>,
    /// The git tree of what the checkout held when the command answered:
    /// what the answer is kept for, so a later invocation can take it for
    /// the same tree. Absent for a check the engine states rather than
    /// runs, and on a log written before the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<TreeId>,
    /// The commit the checkout stood on, kept only for a command that
    /// runs `git`, whose answer can turn on history as well as on files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    /// The line the command concluded with, when it printed one: its last
    /// non-empty stdout line, else its last stderr line. Taken when the
    /// result is recorded, so a replay reads what the live run read.
    /// Absent when nothing was printed, and on a log written before the
    /// field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conclusion: Option<String>,
}

/// When a task's criteria ran: before its work, where each criterion but a
/// guard is meant to be red, or after it, where every one has to pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Pre,
    Post,
}

impl Phase {
    /// The word this phase is called by, as the log spells it — the one
    /// place it is named, so a diagnostic and a surface cannot call the
    /// same phase two things.
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Pre => "pre",
            Phase::Post => "post",
        }
    }
}
