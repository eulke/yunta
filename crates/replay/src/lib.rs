//! What a run is, as its log says: the events folded into a [`RunState`],
//! and every reading of a run derived from that state and its inputs: the
//! frame a surface draws ([`frame`]), the stats ([`stats`]), a workflow's
//! history ([`history`]), the receipt ([`receipt`]), the verification
//! findings ([`verification_effectiveness`]), the nodes a mode schedules
//! ([`modes`]), and what the registry says of the process driving a run, as
//! values ([`registry`]).
//!
//! Pure: nothing here opens a file, reads a clock or spawns a task. An
//! instant, the process driving the run as its registry says, a past run's
//! log: each comes in as an argument its caller read. So a crate that reads a run needs none
//! of the engine that drives one, and a reading at a chosen instant is the
//! same in a test as in production.
//!
//! Its modules stand in layers, each naming only the ones beneath:
//! `modes` and `registry`; replay, at the root; `stats`; `history`; then
//! `receipt`, `verification_effectiveness` and `frame`.
//!
//! With the `testkit` feature, a test of a surface builds the frame it draws
//! from samples beside the frame's types, every field it is not about filled
//! in once.
//!
//! The root's own parts are private: `fold` takes each event in, `refuse`
//! holds the events a log cannot take, `clock` books the run's wall clock,
//! `activity` counts what sessions handed over, `attempt` reads what a node
//! is doing now, `decisions` reads what a person decided about a node, and
//! `tree` says whether the tree moved since a node's failure.
//!
//! [`RunState::fold`] is the functional core: one event taken into the
//! state, no IO, alike for a reader that follows a run and for
//! [`RunState::replay`], which folds a whole log for a property test or
//! for `yunta resume`. It derives node lifecycle
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
//! The fold keeps two rules. An event at or below the state's
//! [`head`](RunState::head) was taken already and moves nothing. And a log
//! that is insufficient or inconsistent — e.g. `node_finished` for a node
//! that was never `node_started` — marks the state `broken` with a
//! diagnostic naming the exact event, rather than panicking or guessing;
//! past it, an event moves the head and nothing else, so a reader never
//! reads the broken log's tail again. There is no corrupt state between
//! the two: the events derive a state, or the run is `broken`. A kind this
//! binary does not know is neither: it is kept by position and name in
//! `unknown_events`, counted by [`RunState::unknown_kinds`], and the fold
//! goes on.
//!
//! Replay reads the events and never their hash chain, which storage keeps
//! and verifies apart.

// A panic is a bug, never a fallible path: production returns a typed
// error instead of unwrapping, expecting, indexing, or panicking.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::indexing_slicing
)]
// Tests are the one place a failed assertion is meant to abort. The panic
// family is lifted there by `clippy.toml`; `indexing_slicing` has no such
// switch, so it is lifted in test builds here. Integration tests are
// separate crates neither reaches.
#![cfg_attr(test, allow(clippy::indexing_slicing))]

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_core::events::artifacts::ArtifactLedger;
use yunta_core::events::findings::FindingLedger;
use yunta_core::events::{
    ChildLedger, DegradationLedger, Failure, Finding, GateLedger, GrantLedger, NodeLedger,
    NodeState, RunLedger, RunMetrics, Spend, TaskLedger, TokenUsage, UnknownKindCount,
};
use yunta_core::{NodeId, RunId, Seq};

pub mod frame;
pub mod history;
pub mod modes;
pub mod receipt;
pub mod registry;
pub mod stats;
pub mod verification_effectiveness;

mod activity;
mod attempt;
mod clock;
mod decisions;
mod fold;
mod refuse;
mod tree;

pub use activity::{FindingActivity, Submissions};
pub use clock::{TimeSpent, Timeline};
pub use refuse::ReplayError;

use activity::Activity;
use clock::Open;

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
    pub unknown_events: Vec<(Seq, String)>,
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
    /// The run this log is of, read off its events. Left out of the comparison: two
    /// runs that derive the same facts are the same state.
    run_id: Option<RunId>,
    /// The seq of the last event folded. It moves past a break while the
    /// ledgers stay at it, and is left out of the comparison.
    head: Option<Seq>,
}

/// Two states are the same when they derive the same facts. The clock, the
/// activity and the head are read beside those facts and are left out of
/// the comparison: a second event stating what the run already holds
/// derives the same facts while the activity counts it, and an event that
/// moves no ledger still moves the head.
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
            && self.unknown_events == other.unknown_events
            && self.broken == other.broken
            && self.pre_gate == other.pre_gate
            && self.tail_paused == other.tail_paused
    }
}

impl RunState {
    /// How many events of each kind this binary does not know, which a
    /// newer engine wrote: grouped by kind and sorted by name, and empty
    /// for a log this binary interprets in full.
    pub fn unknown_kinds(&self) -> Vec<UnknownKindCount> {
        let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for (_, kind) in &self.unknown_events {
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

    /// Each node's share of a person's time, booked as
    /// [`time_spent`](Self::time_spent) books the run's: every stretch a
    /// person was asked about the node, and with `until` the stretch from
    /// the last applied event to it, classified by what is open now. Two
    /// nodes asked at once each hold the stretch the run counts once. A node
    /// no person was asked about is absent.
    pub fn people_at(&self, until: Option<DateTime<Utc>>) -> BTreeMap<NodeId, Duration> {
        let open = Open::of(&self.run, &self.nodes, &self.gates, &self.tasks);
        self.clock.people_at(&open, self.run.suspensions(), until)
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

    /// The run this log is of, once it holds an event.
    pub fn run_id(&self) -> Option<&RunId> {
        self.run_id.as_ref()
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

    /// How many events of `node` the log holds, whatever they are.
    pub fn events_of(&self, node: &NodeId) -> u64 {
        self.activity
            .events_by_node
            .get(node)
            .copied()
            .unwrap_or_default()
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
}

/// Query-side view of `RunState.findings`: findings across reviewers are
/// deduplicated by `location` plus the complaint each makes
/// ([`Finding::complaint`]). The raw log (and `RunState.findings`) keeps
/// every posting; this collapses duplicates for counting/display, keeping
/// the first occurrence — the schema has no authors list to merge into, so
/// authorship is preserved by the untouched event log, not by this derived
/// view.
pub fn dedup_findings(findings: &[Finding]) -> Vec<Finding> {
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::new();
    for finding in findings {
        let key = (finding.location.clone(), finding.complaint());
        if seen.insert(key) {
            deduped.push(finding.clone());
        }
    }
    deduped
}
