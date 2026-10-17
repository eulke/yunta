//! The run's wall clock, booked one stretch at a time as its events apply.
//!
//! Every stretch between two events is the time of one thing: what was
//! open across it — a person being asked, a session waiting on a check, a
//! node's attempt — or, when nothing was, what closed it. Stretches the
//! host slept through are left out, as every other duration leaves them
//! out.
//!
//! A session is without its service from the `service_unreachable` that
//! says it lost it until the `service_reachable` that says it answers
//! again, or its node's end: that wait is nobody's work.
//!
//! A person is being asked from the `asking_opened` the engine writes when
//! it begins asking until the answer the asking ends with — a gate
//! resolved, questions answered — or the invocation stops asking: the run
//! pauses, finishes, or wakes again. Two tasks of a loop working at once
//! share their stretches, and each stretch counts once, under the first
//! bucket of [`TimeSpent`] that it fits.

use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_core::events::{
    CriterionResult, CriterionType, EventPayload, GateLedger, NodeEvent, NodeLedger, Phase,
    RunEvent, RunLedger, RunPhaseRaw, SessionEvent, StoredEvent, Suspensions, TaskEvent,
    TaskLedger,
};

/// A run's wall-clock, by what it was spent on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimeSpent {
    /// A node's attempt open with nothing else holding it: sessions at
    /// work, and the engine's own steps inside a node.
    pub working: Duration,
    /// What sessions waited while the engine formatted what they wrote,
    /// by each format's own measure: taken out of `working`, where it
    /// was spent.
    pub rails: Duration,
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
            + self.rails
            + self.checks
            + self.measuring
            + self.people
            + self.offline
            + self.parked
            + self.between
    }
}

/// The run's timeline: where it began, where its last applied event stands,
/// and every stretch booked between the two.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Timeline {
    first_at: Option<DateTime<Utc>>,
    last_at: Option<DateTime<Utc>>,
    /// The stretches booked so far, by bucket. Write-formatting time is
    /// not in here: it leaves `working` when the clock is read.
    booked: TimeSpent,
    /// How long sessions waited while what they wrote was formatted.
    formatted: Duration,
}

impl Timeline {
    /// When the run's first event was written; `None` for a log with none.
    pub fn first_at(&self) -> Option<DateTime<Utc>> {
        self.first_at
    }

    /// When the last applied event was written; `None` for a log with none.
    pub fn last_at(&self) -> Option<DateTime<Utc>> {
        self.last_at
    }

    /// The span the clock covers up to `until`, or up to the last applied
    /// event when `until` is `None`: from the first event to that end.
    /// `None` for a log with no event.
    pub fn window(&self, until: Option<DateTime<Utc>>) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        self.first_at.zip(until.or(self.last_at))
    }

    /// Books the stretch that ends at `event`, classified by what `open`
    /// holds: the ledgers as they stand before `event` is applied. This runs
    /// before any ledger reads the payload, so a kind this binary does not
    /// know still books its stretch.
    pub(crate) fn book(&mut self, event: &StoredEvent, open: &Open, asleep: &Suspensions) {
        match self.last_at {
            Some(previous) => {
                let stretch = asleep.awake_between(previous, event.timestamp);
                *self.booked.slot(open, event) += stretch;
            }
            None => self.first_at = Some(event.timestamp),
        }
        self.last_at = Some(event.timestamp);
        self.booked.decided_checks += decided(event);
        self.formatted += formatting(event);
    }

    /// Where the wall-clock went up to `until`: the booked stretches, and
    /// the stretch from the last applied event to `until`, classified by
    /// what `open` holds now. Write-formatting time comes out of working
    /// time, where it was spent.
    pub(crate) fn spent(
        &self,
        open: &Open,
        asleep: &Suspensions,
        until: Option<DateTime<Utc>>,
    ) -> TimeSpent {
        let mut spent = self.booked;
        if let (Some(last), Some(until)) = (self.last_at, until) {
            *spent.slot_open(open) += asleep.awake_between(last, until);
        }
        let rails = self.formatted.min(spent.working);
        spent.rails = rails;
        spent.working -= rails;
        spent
    }
}

/// What the ledgers hold open at the moment a stretch closes, read before
/// the event that closes it is applied.
pub(crate) struct Open {
    /// Parked between a pause and the wake that followed it.
    paused: bool,
    /// Some node has an attempt open.
    attempting: bool,
    /// Some node is cut off from its service, and every open attempt
    /// belongs to a node that is.
    attempts_offline: bool,
    /// A person is being asked something, or a node waits on a question
    /// a person answers elsewhere.
    people: bool,
    /// A check of a task's tree is running.
    checking: bool,
}

impl Open {
    /// What the ledgers hold open now.
    pub(crate) fn of(
        run: &RunLedger,
        nodes: &NodeLedger,
        gates: &GateLedger,
        tasks: &TaskLedger,
    ) -> Self {
        Open {
            paused: matches!(run.phase(), RunPhaseRaw::Paused),
            attempting: nodes.attempting(),
            attempts_offline: nodes.attempts_offline(),
            people: !gates.asking().is_empty() || !gates.awaiting().is_empty(),
            checking: !tasks.checking().is_empty(),
        }
    }

    /// Whether nothing is open that a suite measurement waits for.
    fn nothing_running(&self) -> bool {
        !self.paused && !self.attempting && !self.people && !self.checking
    }
}

impl TimeSpent {
    /// The share of the stretch that ends at `closing`.
    fn slot(&mut self, open: &Open, closing: &StoredEvent) -> &mut Duration {
        if open.nothing_running() && is_measurement(closing) {
            return &mut self.measuring;
        }
        if !open.paused && !open.checking && is_check(closing) {
            return &mut self.checks;
        }
        self.slot_open(open)
    }

    /// The share a stretch belongs to by what is open across it.
    fn slot_open(&mut self, open: &Open) -> &mut Duration {
        match () {
            _ if open.paused => &mut self.parked,
            _ if open.people => &mut self.people,
            _ if open.checking => &mut self.checks,
            _ if open.attempts_offline => &mut self.offline,
            _ if open.attempting => &mut self.working,
            _ => &mut self.between,
        }
    }
}

/// How long the format `event` records took; nothing for any other event.
fn formatting(event: &StoredEvent) -> Duration {
    match event.payload() {
        Some(EventPayload::Session(SessionEvent::WriteFormatted(formatted))) => {
            Duration::from_millis(formatted.duration_ms)
        }
        _ => Duration::ZERO,
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
