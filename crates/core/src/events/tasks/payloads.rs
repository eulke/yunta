//! The tasks document, as a run works it: a task registered, a task
//! that reached a new status, and a task session asking how its work
//! would be judged.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::stop::TaskCause;
use crate::events::node::payloads::{Criterion, CriterionResult};
use crate::glob::ScopeGlob;
use crate::hash::{CommitSha, ContentHash};
use crate::ids::{NodeId, Seq, SessionId, TaskId};

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

impl TaskStatus {
    /// The word a task in this status is called by, wherever it is
    /// said — the token the log writes, so a reader who greps a log
    /// for what a surface printed finds it.
    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Ready => "ready",
            TaskStatus::Running => "running",
            TaskStatus::Done => "done",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Failed => "failed",
        }
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
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
    /// On a `blocked`: why the task stopped. Absent on every other
    /// transition, and in a log written before the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<TaskCause>,
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
            stop: None,
        }
    }

    /// A task blocked, why it stopped, and the work its last attempt left,
    /// if it left any. A blocked task always names its stop, so a resumed
    /// run reads the same reason the run that wrote it stopped for.
    pub fn blocked(
        task: TaskId,
        caused_by: Seq,
        left_work: Option<CommitSha>,
        stop: TaskCause,
    ) -> Self {
        TaskStatusChangedPayload {
            left_work,
            stop: Some(stop),
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
            stop: None,
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
    /// The guards the check did not run, by command: one of the task's
    /// own criteria was red, and a guard runs only once every one of
    /// those passes. Absent when every guard ran.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waiting: Vec<String>,
    /// What the work changed outside the task's scope.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outside_scope: Vec<PathBuf>,
    /// What it changed that no session of the run may write: what the
    /// project denies to every run, or a test a person approved.
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
    /// The node that writes the task's tests again, when the person
    /// accepted departing from a test the run's spec gave it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub respecified_by: Option<NodeId>,
}

/// A criterion of a task that its session declared no work of the plan
/// can pass. The engine ran it on the tree the session left and saw it
/// red; the exit code it saw and the session's reason are recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CriterionUnpassablePayload {
    pub task_id: TaskId,
    pub criterion: String,
    pub exit_code: i32,
    pub why: String,
}

/// What the tree a plan was registered against held of the plan's shapes,
/// read in the reading that derives each task's reach, at its commit.
/// Nobody asks for it and nothing is decided by it: every page that draws
/// the plan reads it here, so each says what the tree held when the plan
/// was written, wherever it is drawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ShapesReadPayload {
    /// The plan, by the hash of the bytes the run accepted.
    pub plan: ContentHash,
    /// The commit the tree was read at: its HEAD when the plan was
    /// registered, which a `scope_derived` the same registration writes
    /// names too. A registration that changes no task's reach writes none.
    pub at: CommitSha,
    /// Each shape the reading followed, in the plan's order: one whose name
    /// is one word of code, in a file whose extension says its language.
    pub shapes: Vec<ShapeRead>,
}

impl ShapesReadPayload {
    /// The shape the plan calls `name`, when it was read.
    pub fn shape(&self, name: &str) -> Option<&ShapeRead> {
        self.shapes.iter().find(|shape| shape.name == name)
    }
}

/// One shape of a plan, as the tree held it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ShapeRead {
    /// What the plan calls it; no other shape of the plan has its name.
    pub name: String,
    /// The task that builds it.
    pub owner: TaskId,
    /// Its file, as the repository names it.
    pub file: String,
    /// What its file held of it.
    pub in_tree: InTree,
}

/// What a shape's file held of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InTree {
    /// Its file's code did not name it, or the file did not exist: the plan
    /// makes it. A file with a NUL byte in its first 8000 bytes is not code.
    New,
    /// Its file's code names it.
    There {
        /// What the file holds of the items the shape declares, read the
        /// way the shape writes them: a function to its signature where the
        /// shape writes it with no body, a block of members (an `impl`, a
        /// `trait`, a class) to the members the shape names, attributes and
        /// decorators where the shape writes some; in the shape's order.
        /// Items are read in Rust, Go, JavaScript or TypeScript, and Python.
        /// `None` where no item of it was read: a file in another language,
        /// or one that only uses the name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        /// Who else names it.
        outside: Outside,
    },
}

/// The files beyond a plan whose code names one of its shapes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Outside {
    /// Each file whose code — not a string or a comment in it — names the
    /// shape, other than its own and every file a task of the plan changes,
    /// in path order. None is a fact too.
    Files { paths: Vec<String> },
    /// So many files of its language carry the name — this many — that it
    /// is a word of the language more than a shape of the plan, and none of
    /// them is read to follow it: only the shape's own file is read, for its
    /// code and to see that its code names the shape.
    Common { files: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each state of a shape's file is written by its tag, and read back.
    #[test]
    fn what_a_shape_s_file_held_is_written_by_its_tags() {
        let there = |code: Option<&str>, outside| InTree::There {
            code: code.map(str::to_string),
            outside,
        };
        let files = Outside::Files { paths: Vec::new() };
        for (in_tree, json) in [
            (InTree::New, r#"{"type":"new"}"#),
            (
                there(None, files),
                r#"{"type":"there","outside":{"type":"files","paths":[]}}"#,
            ),
            (
                there(Some("fn a();"), Outside::Common { files: 147 }),
                r#"{"type":"there","code":"fn a();","outside":{"type":"common","files":147}}"#,
            ),
        ] {
            assert_eq!(serde_json::to_string(&in_tree).unwrap(), json);
            assert_eq!(serde_json::from_str::<InTree>(json).unwrap(), in_tree);
        }
    }
}
