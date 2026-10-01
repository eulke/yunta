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
use crate::events::{DeviationDeclaredPayload, DeviationResolvedPayload};
use crate::hash::CommitSha;
use crate::ids::{NodeId, Seq, SessionId, TaskId};

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
    /// The work its last attempt left, and the node whose loop left it:
    /// on a blocked task, what a continuation can pick up; on a task
    /// reopened to continue, what it picks up. Cleared by every status
    /// change that names none.
    pub left_work: Option<(NodeId, CommitSha)>,
    /// The session a task reopened after the answer to its scope request
    /// resumes. Cleared, like `left_work`, by every status change that
    /// names none.
    pub resumes: Option<SessionId>,
    /// The last session that worked this task.
    pub last_session: Option<SessionId>,
    /// A person's answer to the departure from the plan its last session
    /// declared, until the task's next cycle starts with it.
    pub deviation_answer: Option<DeviationResolvedPayload>,
    /// The departures from the plan its sessions declared that no person
    /// has answered. A task that owes one does not close, whatever its
    /// criteria say.
    pub departures_owed: Vec<DeviationDeclaredPayload>,
    /// The departures from the plan a person accepted for it, in the
    /// order they were declared: where the work it did stops being the
    /// plan's.
    pub departures_accepted: Vec<AcceptedDeparture>,
    /// The last time a person accepted that the task's tests are wrong:
    /// the node that writes them again, and what it is told.
    pub respecified: Option<Respecified>,
    /// Whether a session opened for the task since its last status
    /// change. A `running` task without one is having its criteria
    /// checked, or the work it was reopened on judged, before any agent
    /// works it.
    pub in_session: bool,
}

/// A departure from the plan a person accepted, and what they said
/// when they did.
#[derive(Debug, Clone, PartialEq)]
pub struct AcceptedDeparture {
    pub declared: DeviationDeclaredPayload,
    pub said: Option<String>,
}

/// A person's acceptance that a task's tests are wrong: the node that
/// writes them again, where on the log the person answered, the
/// departures they accepted and what they said.
#[derive(Debug, Clone, PartialEq)]
pub struct Respecified {
    pub by: NodeId,
    pub at: Seq,
    pub departures: Vec<DeviationDeclaredPayload>,
    pub said: Option<String>,
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

    /// Whether a task `node`'s loop blocked left work behind — what makes
    /// continuing from that work a choice worth offering.
    pub fn continuable_by(&self, node: &NodeId) -> bool {
        self.per_task.values().any(|record| {
            record.status == TaskStatus::Blocked
                && record.left_work.as_ref().is_some_and(|(by, _)| by == node)
        })
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
            // What a node resolved, started, assembled, hooked, closed,
            // re-routed or opened prices no command.
            NodeEvent::RunnerResolved(_)
            | NodeEvent::Started(_)
            | NodeEvent::ContextAssembled(_)
            | NodeEvent::ScopeChecked(_)
            | NodeEvent::Finished(_)
            | NodeEvent::Failed(_)
            | NodeEvent::HookExecuted(_)
            | NodeEvent::Rerouted(_)
            | NodeEvent::PullRequestOpened(_) => {}
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
                    record.deviation_answer = None;
                }
                record.status = p.new_status;
                if let Some(commit) = &p.commit {
                    record.commit = Some(commit.clone());
                }
                record.left_work = p
                    .left_work
                    .clone()
                    .and_then(|work| meta.node.cloned().map(|node| (node, work)));
                record.resumes = p.resumes.clone();
                record.in_session = false;
                Ok(())
            }
            // A check judges work in progress; the attempt's close moves
            // the task.
            TaskEvent::CheckStarted(_) | TaskEvent::CheckAnswered(_) => Ok(()),
            TaskEvent::DeviationDeclared(p) => {
                let Some(record) = self.per_task.get_mut(&p.task_id) else {
                    return Err(UnknownTask(p.task_id.clone()));
                };
                record.departures_owed.push(p.clone());
                Ok(())
            }
            TaskEvent::DeviationResolved(p) => {
                let Some(record) = self.per_task.get_mut(&p.task_id) else {
                    return Err(UnknownTask(p.task_id.clone()));
                };
                record.answered(p, meta.seq);
                Ok(())
            }
        }
    }

    /// Folds a session a loop opened for one of its tasks: the task's
    /// last session, and one it is in. A session that names no task, or
    /// a task nothing registered, moves nothing.
    pub fn apply_session(&mut self, event: &crate::events::session::kinds::SessionEvent) {
        let crate::events::session::kinds::SessionEvent::Opened(p) = event else {
            return;
        };
        let Some(record) = p
            .task_id
            .as_ref()
            .and_then(|task| self.per_task.get_mut(task))
        else {
            return;
        };
        record.last_session = Some(p.session_id.clone());
        record.in_session = true;
    }
}

impl TaskRecord {
    /// One answer settles every departure the task owed when it was
    /// asked.
    fn answered(&mut self, answer: &DeviationResolvedPayload, at: Seq) {
        let answered = std::mem::take(&mut self.departures_owed);
        if let (true, Some(by)) = (answer.accepted, &answer.respecified_by) {
            self.respecified = Some(Respecified {
                by: by.clone(),
                at,
                departures: answered.clone(),
                said: answer.said.clone(),
            });
        }
        if answer.accepted {
            self.departures_accepted
                .extend(answered.into_iter().map(|declared| AcceptedDeparture {
                    declared,
                    said: answer.said.clone(),
                }));
        }
        self.deviation_answer = Some(answer.clone());
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
            left_work: None,
            resumes: None,
            last_session: None,
            deviation_answer: None,
            departures_owed: Vec::new(),
            departures_accepted: Vec::new(),
            respecified: None,
            in_session: false,
        }
    }
}

/// A status change naming a task no `task_registered` introduced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTask(pub TaskId);
