//! Where a run's wall-clock went, read off its log alone.
//!
//! Every stretch between two events is the time of one thing: what was
//! open across it — a person being asked, a session waiting on a check, a
//! node's attempt — or, when nothing was, what closed it. Stretches the
//! host slept through are left out, as every other duration `stats` gives
//! leaves them out.
//!
//! A session is without its service from the `service_unreachable` that
//! says it lost it until the `service_reachable` that says it answers
//! again, or its node's end: that wait is nobody's work.
//!
//! A person is being asked from the `asking_opened` the engine writes when
//! it begins asking until the answer the asking ends with — a gate
//! resolved, questions answered — or the invocation stops asking: the run
//! pauses, finishes, or wakes again. Two tasks of a loop working at once
//! share their stretches, and each stretch counts once, for the first of
//! its readings below.

use std::collections::HashSet;
use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_core::events::{
    CriterionResult, CriterionType, EventPayload, GateEvent, NodeEvent, Phase, RunEvent,
    SessionEvent, StoredEvent, Suspensions, TaskEvent,
};
use yunta_core::{NodeId, TaskId};

/// A run's wall-clock, by what it was spent on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimeSpent {
    /// A node's attempt open with nothing else holding it: sessions at
    /// work, and the engine's own steps inside a node.
    pub working: Duration,
    /// Criteria and suites the engine ran on a task's tree: before and
    /// after its attempts, and while its session waited on a check.
    pub checks: Duration,
    /// What guards ran while a criterion of their own task was red,
    /// whose answer could not close the task: part of `checks`.
    pub decided_checks: Duration,
    /// The suite measured while nothing else ran.
    pub measuring: Duration,
    /// A person deciding: questions, a gate, an escalation.
    pub people: Duration,
    /// Sessions cut off from their service, waiting for it to answer,
    /// with nothing else at work.
    pub offline: Duration,
    /// Parked between a pause and the wake that followed it.
    pub parked: Duration,
    /// Nothing open: the engine between one step and the next.
    pub between: Duration,
}

impl TimeSpent {
    /// Every stretch, counted once.
    pub fn total(&self) -> Duration {
        self.working
            + self.checks
            + self.measuring
            + self.people
            + self.offline
            + self.parked
            + self.between
    }
}

/// What was open while the log was being read, at the event just read.
#[derive(Default)]
struct Open {
    attempts: HashSet<NodeId>,
    /// Who is being asked a person something, by node: a question to a
    /// person is about a node, or about the run when it names none.
    asking: HashSet<Option<NodeId>>,
    /// Nodes waiting on a question a person answers elsewhere — a gate
    /// published to a forge, a round of questions.
    waiting: HashSet<NodeId>,
    checking: HashSet<TaskId>,
    /// Nodes whose session lost its service and waits for it.
    offline: HashSet<NodeId>,
    paused: bool,
}

/// Where `events`' wall-clock went, up to `until` — the log's last event,
/// or later for a run still going.
pub fn time_spent(
    events: &[StoredEvent],
    asleep: &Suspensions,
    until: Option<DateTime<Utc>>,
) -> TimeSpent {
    let mut spent = TimeSpent::default();
    let mut open = Open::default();
    for (index, event) in events.iter().enumerate() {
        if let Some(previous) = index.checked_sub(1).and_then(|at| events.get(at)) {
            let stretch = asleep.awake_between(previous.timestamp, event.timestamp);
            *spent.slot(&open, event) += stretch;
        }
        open.read(event);
        spent.decided_checks += decided(event);
    }
    if let (Some(last), Some(until)) = (events.last(), until) {
        let stretch = asleep.awake_between(last.timestamp, until);
        *spent.slot_open(&open) += stretch;
    }
    spent
}

impl TimeSpent {
    /// The share the stretch ending at `closing` belongs to.
    fn slot(&mut self, open: &Open, closing: &StoredEvent) -> &mut Duration {
        if !open.paused && !open.is_busy() && is_measurement(closing) {
            return &mut self.measuring;
        }
        if !open.paused && open.checking.is_empty() && is_check(closing) {
            return &mut self.checks;
        }
        self.slot_open(open)
    }

    /// The share a stretch belongs to by what is open across it.
    fn slot_open(&mut self, open: &Open) -> &mut Duration {
        match () {
            _ if open.paused => &mut self.parked,
            _ if !open.asking.is_empty() || !open.waiting.is_empty() => &mut self.people,
            _ if !open.checking.is_empty() => &mut self.checks,
            _ if !open.offline.is_empty() && open.attempts.is_subset(&open.offline) => {
                &mut self.offline
            }
            _ if !open.attempts.is_empty() => &mut self.working,
            _ => &mut self.between,
        }
    }
}

impl Open {
    /// Whether anything at all is open.
    fn is_busy(&self) -> bool {
        !self.attempts.is_empty()
            || !self.asking.is_empty()
            || !self.waiting.is_empty()
            || !self.checking.is_empty()
    }

    fn read(&mut self, event: &StoredEvent) {
        let node = event.node_id.clone();
        match event.payload() {
            Some(EventPayload::Node(NodeEvent::Started(_))) => {
                self.attempts.extend(node);
            }
            Some(EventPayload::Node(NodeEvent::Finished(_) | NodeEvent::Failed(_))) => {
                if let Some(node) = &node {
                    self.attempts.remove(node);
                    self.offline.remove(node);
                }
            }
            Some(EventPayload::Session(SessionEvent::ServiceUnreachable(_))) => {
                self.offline.extend(node);
            }
            Some(EventPayload::Session(SessionEvent::ServiceReachable(_))) => {
                if let Some(node) = &node {
                    self.offline.remove(node);
                }
            }
            Some(EventPayload::Gates(gate)) => self.read_gate(gate, node),
            Some(EventPayload::Tasks(TaskEvent::CheckStarted(check))) => {
                self.checking.insert(check.task_id.clone());
            }
            Some(EventPayload::Tasks(TaskEvent::CheckAnswered(check))) => {
                self.checking.remove(&check.task_id);
            }
            Some(EventPayload::Run(RunEvent::Paused(_))) => {
                self.asking.clear();
                self.paused = true;
            }
            Some(EventPayload::Run(RunEvent::Resumed(_) | RunEvent::Finished(_))) => {
                self.asking.clear();
                self.paused = false;
            }
            _ => {}
        }
    }

    fn read_gate(&mut self, gate: &GateEvent, node: Option<NodeId>) {
        match gate {
            GateEvent::AskingOpened(_) => {
                self.asking.insert(node);
            }
            GateEvent::QuestionsAsked(_) => self.waiting.extend(node),
            GateEvent::Waiting(waiting) if waiting.external_ref().is_some() => {
                self.waiting.extend(node);
            }
            GateEvent::Waiting(_) => {}
            GateEvent::Resolved(_) | GateEvent::QuestionsAnswered(_) => {
                self.asking.remove(&node);
                if let Some(node) = &node {
                    self.waiting.remove(node);
                }
            }
        }
    }
}

/// Whether `event` is the suite's measurement, closing a stretch it ran.
fn is_measurement(event: &StoredEvent) -> bool {
    matches!(
        event.payload(),
        Some(EventPayload::Run(RunEvent::BaselineCaptured(_)))
    )
}

/// Whether `event` closes a check the engine ran on a task's tree.
fn is_check(event: &StoredEvent) -> bool {
    matches!(
        event.payload(),
        Some(EventPayload::Node(NodeEvent::CriteriaChecked(_)))
            | Some(EventPayload::Tasks(TaskEvent::CheckAnswered(_)))
    )
}

/// What a check after the work ran its guards for nothing: a criterion of
/// the task's own was red in the same check, so the task could not close
/// whatever they answered. Before the work its own criteria are meant to
/// be red, and a guard there is asked whether the tree it starts from
/// holds.
fn decided(event: &StoredEvent) -> Duration {
    let results: &[CriterionResult] = match event.payload() {
        Some(EventPayload::Node(NodeEvent::CriteriaChecked(check)))
            if check.phase == Phase::Post =>
        {
            &check.results
        }
        Some(EventPayload::Tasks(TaskEvent::CheckAnswered(check))) => &check.results,
        _ => return Duration::ZERO,
    };
    let guard = |result: &CriterionResult| result.r#type == Some(CriterionType::Guard);
    let own_red = results
        .iter()
        .any(|result| !guard(result) && result.exit_code != 0);
    if !own_red {
        return Duration::ZERO;
    }
    results
        .iter()
        .filter(|result| guard(result) && !result.reused)
        .filter_map(|result| result.duration_ms)
        .map(Duration::from_millis)
        .sum()
}
