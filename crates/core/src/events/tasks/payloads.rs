//! The tasks document, as a run works it: a task registered, and a task
//! that reached a new status.

use serde::{Deserialize, Serialize};

use crate::events::node::payloads::Criterion;
use crate::hash::CommitSha;
use crate::ids::{Seq, TaskId};

/// Exact variant names are provisional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Ready,
    Running,
    Done,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskRegisteredPayload {
    pub task_id: TaskId,
    pub criteria: Vec<Criterion>,
    pub scope: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<TaskId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskStatusChangedPayload {
    pub task_id: TaskId,
    pub new_status: TaskStatus,
    /// `seq` of the event that justifies this transition.
    pub caused_by: Seq,
    /// Where the task's work landed, on a `done` and nowhere else: the
    /// commit the run's tree carried after integrating it. What makes a
    /// `done` answerable by another run — a tree either descends from
    /// this commit or does not have the work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<CommitSha>,
}
