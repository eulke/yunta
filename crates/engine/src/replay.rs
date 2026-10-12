//! State derivation by replay: a run's whole state read back off its
//! event log.
//!
//! `derive` is the functional core: a pure function over an event slice,
//! no IO, safe to call from a property test or from `yunta resume`
//! alike. It derives node lifecycle
//! (`node_started`/`node_finished`/`node_failed`), task status and the
//! node each task belongs to (`task_registered`/`task_status_changed`),
//! what the run has spent, which `limits.max_tokens_per_run` is compared
//! against, and the wall clock the run has used. `waiting` is derived
//! too: a published gate without its resolution (the state that outlives
//! an invocation), and a node that recorded `questions_asked` with no
//! `questions_answered` beside it.
//! Both are the same shape — a fact that opens the wait and one that
//! closes it — and both round-trip back to the node's prior state by
//! construction, so a node that asked comes back `Running`, owed the
//! terminal its close deferred.
//!
//! A log that is insufficient or inconsistent — e.g. `node_finished` for a
//! node that was never `node_started` — marks the result `broken` with a
//! diagnostic naming the exact event, rather than panicking or guessing.
//! Replay stops at the first such event; the state accumulated up to
//! that point is still returned.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_core::events::artifacts::ArtifactLedger;
use yunta_core::events::findings::FindingLedger;
use yunta_core::events::tasks::ledger::UnknownTask;
use yunta_core::events::{
    ChildLedger, DegradationLedger, EventMeta, EventPayload, Failure, Finding, GateLedger,
    GrantLedger, NodeLedger, RunEvent, RunLedger, RunMetrics, Spend, StoredEvent, TaskLedger,
    TokenUsage,
};
use yunta_core::{NodeId, Seq};

/// Re-exported where it has always been read from: the node states this
/// module derives are `yunta_core::events`' to define now, and every
/// caller keeps naming them here.
pub use yunta_core::events::{NodeState, NodeWait};

mod activity;
mod clock;
mod decisions;
mod refuse;

pub use clock::TimeSpent;
pub use refuse::ReplayError;

use activity::Activity;
use clock::{Open, Timeline};

/// Every fold of a run's log, in one value.
///
/// One ledger per domain, each the single answer to what its own kinds
/// mean. A surface that counts attempts, lists findings, asks what a
/// task is doing or where a gate stands reads the ledger rather than
/// walking the log with a rule of its own.
#[derive(Debug, Clone, Default)]
pub struct RunState {
    pub run: RunLedger,
    pub nodes: NodeLedger,
    pub degradations: DegradationLedger,
    pub tasks: TaskLedger,
    pub grants: GrantLedger,
    pub findings: FindingLedger,
    pub artifacts: ArtifactLedger,
    pub gates: GateLedger,
    pub children: ChildLedger,
    /// Every event under a `kind` this binary does not know, by position
    /// and kind name, in log order: the run is interpreted up to what
    /// this binary understands, and what it skipped is named.
    pub unknown_kinds: Vec<(Seq, String)>,
    /// `Some(diagnostic)` once the log has proven insufficient to derive
    /// further state — the point where a `yunta resume`/`status` would
    /// report the run as `broken`.
    pub broken: Option<String>,
    /// What a node was before the fact that opened its wait, so the fact
    /// that closes it restores exactly that: the internal gate pair
    /// leaves a `Failed` node `Failed`, and a node that asked comes back
    /// `Running`.
    pre_gate: HashMap<NodeId, Option<NodeState>>,
    /// The run's wall clock: its first event, its last applied event, and
    /// every stretch booked between them. Boxed, with the activity, so a
    /// state held across the awaits of a run stays the size of its facts.
    clock: Box<Timeline>,
    /// Submissions and finding calls the log carries, for the run and for
    /// each node that made one.
    activity: Box<Activity>,
    /// Whether the last event applied was a pause.
    tail_paused: bool,
}

/// Two states are the same when they derive the same facts. The clock and
/// the activity are read beside those facts and are left out of the
/// comparison: an event delivered twice derives the same facts, while the
/// activity counts both deliveries.
impl PartialEq for RunState {
    fn eq(&self, other: &Self) -> bool {
        self.run == other.run
            && self.nodes == other.nodes
            && self.degradations == other.degradations
            && self.tasks == other.tasks
            && self.grants == other.grants
            && self.findings == other.findings
            && self.artifacts == other.artifacts
            && self.gates == other.gates
            && self.children == other.children
            && self.unknown_kinds == other.unknown_kinds
            && self.broken == other.broken
            && self.pre_gate == other.pre_gate
            && self.tail_paused == other.tail_paused
    }
}

impl RunState {
    /// Whether an invocation already woke this run.
    ///
    /// A birth writes as many events as the run was born holding — what
    /// the run is, the artifacts and tasks it starts with, the
    /// measurement its lineage handed it — and none of them is a wake.
    /// A wake leaves a pause, a resume or a measurement this run took;
    /// an invocation that died without writing its pause leaves only
    /// the nodes it started. What separates a first wake from a resume
    /// is what the log says happened, never how much of it there is.
    ///
    /// It reads more than one ledger because the question is about the
    /// whole log: each ledger answers for its own kinds, and this is
    /// the one place that reads them together.
    pub fn woken(&self) -> bool {
        self.run.woken()
            || !self.nodes.is_empty()
            // A log replay stopped on holds an event a birth never
            // writes; something worked on this run, whatever else is
            // true of what it left behind.
            || self.broken.is_some()
    }

    /// What the run has spent. A closed run answers with what its close
    /// recorded, and nothing is in flight once it has closed; an open one
    /// sums every node's spend and every child run's total.
    pub fn spend(&self) -> Spend {
        match self.run.finished() {
            Some(finished) => Spend {
                settled: finished.metrics.tokens,
                in_flight: TokenUsage::default(),
            },
            None => {
                self.nodes
                    .values()
                    .map(|record| record.spend)
                    .sum::<Spend>()
                    + Spend {
                        settled: self.children.tokens(),
                        in_flight: TokenUsage::default(),
                    }
            }
        }
    }

    /// The run's metrics as its close would record them at this point.
    pub fn metrics(&self) -> RunMetrics {
        RunMetrics::of(self.spend().so_far(), self.tasks.done())
    }

    /// Where the run's wall-clock went. With `until`, the stretch from the
    /// last applied event to it is counted too, classified by what is open
    /// now.
    pub fn time_spent(&self, until: Option<DateTime<Utc>>) -> TimeSpent {
        let open = Open::of(&self.run, &self.nodes, &self.gates, &self.tasks);
        self.clock.spent(&open, self.run.suspensions(), until)
    }

    /// The awake time from the first event to `until`, or to the last
    /// applied event when `until` is `None`. `None` for a log with no event.
    pub fn elapsed(&self, until: Option<DateTime<Utc>>) -> Option<Duration> {
        let (first, end) = self.clock.window(until)?;
        Some(self.run.suspensions().awake_between(first, end))
    }

    /// Whether the run has closed, or its log has broken before it could.
    pub fn ended(&self) -> bool {
        self.run.finished().is_some() || self.broken.is_some()
    }

    /// The failure the run stands on: the one of the node still in
    /// `Failed` that failed last.
    pub fn last_failure(&self) -> Option<Failure> {
        self.nodes
            .values()
            .filter_map(|record| match (&record.state, record.last_failed) {
                (Some(NodeState::Failed { failure, .. }), Some(seq)) => Some((seq, failure)),
                _ => None,
            })
            .max_by_key(|(seq, _)| *seq)
            .map(|(_, failure)| failure.clone())
    }

    /// Whether the last event applied was a pause.
    pub fn tail_paused(&self) -> bool {
        self.tail_paused
    }

    /// The run's clock: its first and last event, and every stretch booked
    /// between them.
    pub fn clock(&self) -> &Timeline {
        &self.clock
    }

    /// What sessions handed the run and what the engine answered.
    pub(crate) fn activity(&self) -> &Activity {
        &self.activity
    }

    /// Every finding that stands, in the order each was first posted.
    /// Never deduplicated here — two nodes that find the same thing each
    /// keep their own posting; [`dedup_findings`] is the query-side view
    /// for counting and display.
    pub fn effective_findings(&self) -> Vec<Finding> {
        self.findings
            .effective()
            .into_iter()
            .map(|posted| posted.finding)
            .collect()
    }

    /// Every finding that stands and nothing settled: what still counts.
    pub fn unsettled_findings(&self) -> Vec<Finding> {
        let unsettled = self.findings.unsettled().into_iter();
        unsettled.map(|posted| posted.finding).collect()
    }

    /// Whether `node`'s questions were answered and its close still owes
    /// it a terminal.
    ///
    /// The node is `Running` again — its close already ran, when it
    /// asked — so nothing tells it apart from a node a crash orphaned
    /// except this.
    pub fn answered_unfinished(&self, node: &NodeId) -> bool {
        let answered = self.gates.answered(node);
        let owed = self.nodes.get(node).is_some_and(|record| {
            let answered_at = self
                .gates
                .get(node)
                .and_then(|gate| gate.rounds.last())
                .and_then(|round| round.answered_at);
            match (answered_at, record.last_terminal) {
                (Some(answered), Some(terminal)) => terminal < answered,
                (Some(_), None) => true,
                _ => false,
            }
        });
        answered && owed
    }

    /// Whether nothing has been derived from this state's log beyond the
    /// fact that a node was heard from — a node record carrying a
    /// timestamp and no derivation behind it.
    ///
    /// What a kind of event states about the run, told apart from the
    /// envelope every event carries: `a_kind_that_moves_no_state_says_so_by_name`
    /// pairs this with an explicit list of the kinds that move no state.
    /// The clock and the activity are not compared, as in equality.
    pub fn derives_nothing(&self) -> bool {
        let heard_from_only = self.nodes.iter().all(|(_, record)| {
            *record
                == yunta_core::events::NodeRecord {
                    last_event_at: record.last_event_at,
                    ..Default::default()
                }
        });
        let rest = RunState {
            nodes: NodeLedger::default(),
            ..self.clone()
        };
        heard_from_only && rest == RunState::default()
    }

    /// Folds one event into every ledger its domain reaches.
    ///
    /// The order is fixed. The event is refused first, so a refused event
    /// moves nothing and books no time. A suspension the event states is
    /// recorded next, before the stretch it ends is booked, so that
    /// stretch leaves the sleep out. The stretch is then booked against
    /// what the ledgers hold open, and each owner folds the event. The
    /// activity counts it last.
    ///
    /// The dispatch names every domain: a kind this binary knows and no
    /// ledger reads would be a kind that silently derives nothing, which
    /// is the failure this shape exists to make impossible.
    pub fn apply(&mut self, event: &StoredEvent) -> Result<(), ReplayError> {
        self.refuse(event)?;
        let meta = EventMeta::of(event);
        if let Some(EventPayload::Run(e @ RunEvent::HostSuspended(_))) = event.payload() {
            self.run.apply(e, &meta);
        }
        let open = Open::of(&self.run, &self.nodes, &self.gates, &self.tasks);
        self.clock.book(event, &open, self.run.suspensions());
        self.apply_owners(event, &meta)?;
        self.activity.apply(event);
        self.tail_paused = matches!(
            event.payload(),
            Some(EventPayload::Run(RunEvent::Paused(_)))
        );
        Ok(())
    }

    /// Hands the event to every ledger that reads its domain.
    fn apply_owners(
        &mut self,
        event: &StoredEvent,
        meta: &EventMeta<'_>,
    ) -> Result<(), ReplayError> {
        let Some(payload) = event.payload() else {
            // A kind this binary does not know: counted, named, never a
            // reason to stop deriving what it does know.
            self.unknown_kinds
                .push((event.seq, event.body.kind_name().to_string()));
            return Ok(());
        };
        match payload {
            EventPayload::Run(e) => self.apply_run(e, meta),
            EventPayload::Node(e) => {
                self.nodes.apply(e, meta, self.run.suspensions());
                self.tasks.apply_criteria(e);
            }
            EventPayload::Session(e) => {
                self.nodes
                    .apply_session(e, meta)
                    .map_err(|_| ReplayError::SessionWithoutNode {
                        seq: event.seq,
                        kind: event.body.kind_name().to_string(),
                    })?;
                self.tasks.apply_session(e);
                self.degradations.apply(e, meta);
            }
            EventPayload::Tasks(e) => self.tasks.apply(e, meta).map_err(|UnknownTask(task)| {
                ReplayError::StatusWithoutTask {
                    seq: event.seq,
                    task,
                }
            })?,
            EventPayload::Scope(e) => self
                .grants
                .apply(e, meta)
                .map_err(|_| ReplayError::UngrantableSubject { seq: event.seq })?,
            EventPayload::Findings(e) => self.findings.apply(event.node_id.as_ref(), e),
            EventPayload::Artifacts(e) => {
                self.artifacts.apply(event.node_id.as_ref(), event.seq, e);
            }
            EventPayload::Gates(e) => {
                self.gate_transition(e, event, meta)?;
                self.gates.apply(e, meta);
            }
            EventPayload::Children(e) => self.children.apply(e, meta),
        }
        Ok(())
    }

    /// The run's own events. A host suspension was recorded before the clock
    /// booked its stretch, so it is not folded again here; a pause, a resume
    /// or a close ends every question the run still holds.
    fn apply_run(&mut self, event: &RunEvent, meta: &EventMeta<'_>) {
        if matches!(event, RunEvent::HostSuspended(_)) {
            return;
        }
        self.run.apply(event, meta);
        if matches!(
            event,
            RunEvent::Paused(_) | RunEvent::Resumed(_) | RunEvent::Finished(_)
        ) {
            self.gates.clear_asking();
        }
    }
}

/// A run's log read once and its [`RunState`] derived once — the pair
/// almost every read site needs together. The read and the [`derive`]
/// call live here, in one place, instead of being repeated at each site.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunView {
    pub(crate) events: Vec<StoredEvent>,
    /// Boxed: a view is held across the awaits of every run step, and the
    /// state is the bulk of it, so the futures of those steps stay small.
    pub(crate) state: Box<RunState>,
}

impl RunView {
    /// Derives the state from the events, keeping both.
    pub(crate) fn of(events: Vec<StoredEvent>) -> Self {
        let state = Box::new(derive(&events));
        Self { events, state }
    }
}

pub use yunta_core::events::UnknownKindCount;

/// The unknown kinds a derived state skipped, grouped by kind and sorted
/// by name; empty for a log this binary interprets in full.
pub fn unknown_kind_counts(state: &RunState) -> Vec<UnknownKindCount> {
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for (_, kind) in &state.unknown_kinds {
        *counts.entry(kind.as_str()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(kind, events)| UnknownKindCount {
            kind: kind.to_string(),
            events,
        })
        .collect()
}

/// Derives run state from its event log, in `seq` order. Pure: same
/// input, same output, always — a property test relies on exactly
/// this.
pub fn derive(events: &[StoredEvent]) -> RunState {
    let mut state = RunState::default();
    for event in events {
        if let Err(error) = state.apply(event) {
            state.broken = Some(error.to_string());
            break;
        }
    }
    state
}

/// Query-side view of `RunState.findings`: findings across reviewers are
/// deduplicated by `location` plus normalized title. The raw log (and
/// `RunState.findings`) keeps every posting; this collapses duplicates
/// for counting/display, keeping the first occurrence — the schema has
/// no authors list to merge into, so authorship is preserved by the
/// untouched event log, not by this derived view.
pub fn dedup_findings(findings: &[Finding]) -> Vec<Finding> {
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::new();
    for finding in findings {
        let key = (finding.location.clone(), normalized_title(&finding.title));
        if seen.insert(key) {
            deduped.push(finding.clone());
        }
    }
    deduped
}

/// Case- and whitespace-insensitive: "Scope  expansion DENIED" and
/// "scope expansion denied" are the same complaint about the same place.
fn normalized_title(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
