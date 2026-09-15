//! The tasks document as a run works it, folded once.
//!
//! Five places used to walk the task events with their own rule: one for
//! the status, one for the attempt number, one for the commit a `done`
//! landed at, one to find the registration a status change is caused by.
//! Every surface that asks what a task is doing — or what the criteria
//! it is checked against cost — reads it here rather than walking the
//! kinds itself: a second fold is a second answer.

use std::collections::BTreeMap;

use crate::events::meta::EventMeta;
use crate::events::node::kinds::NodeEvent;
use crate::events::tasks::kinds::TaskEvent;
use crate::events::TaskStatus;
use crate::hash::CommitSha;
use crate::ids::{NodeId, Seq, TaskId};

/// What the log says about one task.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskRecord {
    /// Where the task stands. A registered task nothing has moved is
    /// `Pending`, which is what `task_registered` alone means.
    pub status: TaskStatus,
    /// The node whose events registered it. Two `loop` nodes running at
    /// once each register their own tasks, and this is what tells the
    /// sets apart. The first node the log names keeps it: a later status
    /// change never re-homes a task.
    pub owner: Option<NodeId>,
    /// Where the registration sits — what a status change names as the
    /// event that justifies it.
    pub registered_at: Option<Seq>,
    /// How many times this task was dispatched: one per move into
    /// `Running`.
    pub attempts: u32,
    /// The commit its work landed at, which only a `done` names.
    pub commit: Option<CommitSha>,
}

/// Every task's record, by id, and what the criteria they were checked
/// against cost.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TaskLedger {
    per_task: BTreeMap<TaskId, TaskRecord>,
    /// What each criterion command cost, keyed by the command itself:
    /// the cost belongs to the command, not to the task that named it,
    /// so one suite guarding twenty tasks has one history.
    per_criterion: BTreeMap<String, Vec<u64>>,
}

impl TaskLedger {
    /// What the log says about `task`.
    pub fn get<Q>(&self, task: &Q) -> Option<&TaskRecord>
    where
        TaskId: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.per_task.get(task)
    }

    /// `task`'s status; `None` for a task the log never registered.
    pub fn status<Q>(&self, task: &Q) -> Option<TaskStatus>
    where
        TaskId: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.per_task.get(task).map(|record| record.status)
    }

    /// Whether the log registered `task`.
    pub fn contains<Q>(&self, task: &Q) -> bool
    where
        TaskId: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.per_task.contains_key(task)
    }

    /// Every task the log names, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (&TaskId, &TaskRecord)> {
        self.per_task.iter()
    }

    /// How many tasks the log registered.
    pub fn len(&self) -> usize {
        self.per_task.len()
    }

    /// Whether the log registered any task at all.
    pub fn is_empty(&self) -> bool {
        self.per_task.is_empty()
    }

    /// Every task's status, in id order.
    pub fn statuses(&self) -> impl Iterator<Item = TaskStatus> + '_ {
        self.per_task.values().map(|record| record.status)
    }

    /// How many tasks reached `done` — the denominator of cost per
    /// verified task.
    pub fn done(&self) -> usize {
        self.per_task
            .values()
            .filter(|record| matches!(record.status, TaskStatus::Done))
            .count()
    }

    /// Every wall-clock duration the log records for the criterion
    /// command `cmd`, in the order the run measured them.
    ///
    /// What a command costs is the only thing an execution order can be
    /// learned from, and the log is where it lives: a wake reads what
    /// the wakes before it measured instead of measuring again. A
    /// command the log never timed reads as empty — one no task
    /// declares, and one every check answered out of an invocation's
    /// result cache without running it.
    pub fn criterion_durations(&self, cmd: &str) -> &[u64] {
        self.per_criterion.get(cmd).map_or(&[], Vec::as_slice)
    }

    /// Folds one node-domain event into what it says about the tasks: a
    /// `criteria_checked` prices every criterion it actually ran, and
    /// that price outlives the invocation that paid it.
    pub fn apply_criteria(&mut self, event: &NodeEvent) {
        match event {
            NodeEvent::CriteriaChecked(p) => {
                for result in &p.results {
                    // A reused result timed nothing — the invocation
                    // answered it from its own cache — so what the
                    // command costs stays what its executions measured.
                    if let Some(duration_ms) = result.duration_ms {
                        self.per_criterion
                            .entry(result.cmd.clone())
                            .or_default()
                            .push(duration_ms);
                    }
                }
            }
            // What a node resolved, started, assembled, hooked, closed
            // or re-routed prices no command.
            NodeEvent::RunnerResolved(_)
            | NodeEvent::BaselineCaptured(_)
            | NodeEvent::Started(_)
            | NodeEvent::ContextAssembled(_)
            | NodeEvent::ScopeChecked(_)
            | NodeEvent::Finished(_)
            | NodeEvent::Failed(_)
            | NodeEvent::HookExecuted(_)
            | NodeEvent::Rerouted(_) => {}
        }
    }

    /// Folds one task-domain event. A status change for a task nothing
    /// registered is refused: the caller turns that into the broken log
    /// it is.
    pub fn apply(&mut self, event: &TaskEvent, meta: &EventMeta<'_>) -> Result<(), UnknownTask> {
        match event {
            TaskEvent::Registered(p) => {
                let record = self.per_task.entry(p.task_id.clone()).or_default();
                record.owner = record.owner.take().or_else(|| meta.node.cloned());
                record.registered_at.get_or_insert(meta.seq);
                Ok(())
            }
            TaskEvent::StatusChanged(p) => {
                let Some(record) = self.per_task.get_mut(&p.task_id) else {
                    return Err(UnknownTask(p.task_id.clone()));
                };
                record.owner = record.owner.take().or_else(|| meta.node.cloned());
                if matches!(p.new_status, TaskStatus::Running) {
                    record.attempts += 1;
                }
                record.status = p.new_status;
                if let Some(commit) = &p.commit {
                    record.commit = Some(commit.clone());
                }
                Ok(())
            }
        }
    }
}

impl Default for TaskRecord {
    fn default() -> Self {
        TaskRecord {
            status: TaskStatus::Pending,
            owner: None,
            registered_at: None,
            attempts: 0,
            commit: None,
        }
    }
}

/// A status change naming a task no `task_registered` introduced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTask(pub TaskId);
