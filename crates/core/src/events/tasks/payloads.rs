//! The tasks document, as a run works it: a task registered, a task
//! that reached a new status, and a task session asking how its work
//! would be judged.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::events::node::payloads::{Criterion, CriterionResult};
use crate::glob::ScopeGlob;
use crate::hash::CommitSha;
use crate::ids::{Seq, SessionId, TaskId};

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
    pub scope: Vec<ScopeGlob>,
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
    /// The commit holding the work the task's last attempt left: on a
    /// `blocked`, work a person can have the next cycle continue from;
    /// on a `pending` a person reopened that way, the work it continues
    /// from. Absent when the attempt left nothing, and on every other
    /// transition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_work: Option<CommitSha>,
    /// On a `pending` that reopens a task after the answer to a scope
    /// request its session made: that session, which the next cycle
    /// resumes on the same work. Absent on every other transition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumes: Option<SessionId>,
}

impl TaskStatusChangedPayload {
    /// A task moved to `status`, justified by the event at `caused_by`.
    ///
    /// No commit: a status other than `done` names none, because only
    /// finished work has landed anywhere. A `done` goes through
    /// [`TaskStatusChangedPayload::done`], which requires one.
    pub fn to(task: TaskId, status: TaskStatus, caused_by: Seq) -> Self {
        TaskStatusChangedPayload {
            task_id: task,
            new_status: status,
            caused_by,
            commit: None,
            left_work: None,
            resumes: None,
        }
    }

    /// A task blocked, and the work its last attempt left, if it left any.
    pub fn blocked(task: TaskId, caused_by: Seq, left_work: Option<CommitSha>) -> Self {
        TaskStatusChangedPayload {
            left_work,
            ..Self::to(task, TaskStatus::Blocked, caused_by)
        }
    }

    /// A blocked task reopened to continue from the work at `from`.
    pub fn continuing(task: TaskId, caused_by: Seq, from: CommitSha) -> Self {
        TaskStatusChangedPayload {
            left_work: Some(from),
            ..Self::to(task, TaskStatus::Pending, caused_by)
        }
    }

    /// A blocked task reopened, after the answer to the scope its session
    /// asked for, to resume that `session` on the work at `from`.
    pub fn resuming(task: TaskId, caused_by: Seq, from: CommitSha, session: SessionId) -> Self {
        TaskStatusChangedPayload {
            resumes: Some(session),
            ..Self::continuing(task, caused_by, from)
        }
    }

    /// A task finished, and the commit its work landed at.
    ///
    /// The commit is what makes a `done` answerable by another run: a
    /// tree either descends from it or does not have the work. A `done`
    /// without one is a claim no successor can check, which is why this
    /// is the only way to write one.
    pub fn done(task: TaskId, caused_by: Seq, commit: CommitSha) -> Self {
        TaskStatusChangedPayload {
            task_id: task,
            new_status: TaskStatus::Done,
            caused_by,
            commit: Some(commit),
            left_work: None,
            resumes: None,
        }
    }
}

/// A task session asked the engine to judge its work as it stands — the
/// judgement the attempt's close would make if the session ended now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskCheckStartedPayload {
    pub task_id: TaskId,
}

/// What that judgement answered the session, and how long it took.
///
/// A `task_check_started` with no answer after it is a check whose
/// session ended first, or whose answer never reached it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskCheckAnsweredPayload {
    pub task_id: TaskId,
    /// Whether the task would have been done had the session ended then.
    pub closes: bool,
    pub results: Vec<CriterionResult>,
    /// What the work changed outside the task's scope.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outside_scope: Vec<PathBuf>,
    /// What it changed that the project denies to every run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied: Vec<PathBuf>,
    pub duration_ms: u64,
}

/// What of the plan a task session departs from: one of the plan's
/// shapes or decisions by name, one of its own changes by where it is,
/// one of its criteria by its command, or what it said a person would
/// see once it was done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DepartsFrom {
    Shape(String),
    Decision(String),
    Change(String),
    Criterion(String),
    Outcome,
}

impl std::fmt::Display for DepartsFrom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DepartsFrom::Shape(name) => write!(f, "shape `{name}`"),
            DepartsFrom::Decision(id) => write!(f, "decision `{id}`"),
            DepartsFrom::Change(at) => write!(f, "the change at `{at}`"),
            DepartsFrom::Criterion(cmd) => write!(f, "the criterion `{cmd}`"),
            DepartsFrom::Outcome => f.write_str("the task's outcome"),
        }
    }
}

/// A task session declared that its work departs from the plan: what
/// the plan said, what the session did or needs instead, and why. The
/// task does not close on it until a person answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeviationDeclaredPayload {
    pub task_id: TaskId,
    pub from: DepartsFrom,
    /// What the plan says, in the session's words.
    pub planned: String,
    /// What the work does or needs instead.
    pub instead: String,
    pub why: String,
}

/// A person's answer to what a task's session departed from the plan
/// on: accepted as it stands, or sent back with what they said.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeviationResolvedPayload {
    pub task_id: TaskId,
    pub accepted: bool,
    /// What the person said, when they said anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
}
