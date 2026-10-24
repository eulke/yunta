//! Every node's lifecycle, folded once: what an attempt is, when a session
//! is open, and which failure is the current one. Every surface that counts
//! attempts, reports a node's state, times it or lists what it ran reads
//! that here rather than folding the `node_started`/`node_finished`/
//! `node_failed` triple itself — with three kinds per attempt, a second
//! fold is a second answer.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::events::meta::EventMeta;
use crate::events::node::kinds::NodeEvent;
use crate::events::node::payloads::{
    DiscardedCandidate, NodeOutcome, NodeReroutedPayload, RerouteOrigin, RunnerResolvedPayload,
};
use crate::events::{Failure, Spend, Suspensions, TokenUsage};
use crate::ids::{NodeId, QuestionId, RunnerName, Seq};
use crate::{NonEmpty, RunnerCandidate, TreeId};

mod sessions;
mod spend;
mod standing;

use standing::Tally;

pub use sessions::{OpenSession, OrphanedSession, RefusedStop, RefusedWrite, ToolCall};

/// One node's derived lifecycle state. An enum, not booleans: there is
/// no combination of flags to get wrong.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeState {
    Running {
        attempt: u32,
    },
    Finished {
        outcome: NodeOutcome,
        tokens: TokenUsage,
    },
    Failed {
        /// Why the node failed, as the log recorded it. The words a
        /// reader sees are written from this, so no surface can disagree
        /// with the facts behind it.
        failure: Failure,
        tokens: TokenUsage,
    },
    /// Waiting on a human, and on what.
    Waiting {
        on: NodeWait,
    },
}

/// What a waiting node waits on.
///
/// A gate's kind — internal or external — is a property of the
/// declaration, never of the wait: the scheduler reads it from the
/// workflow, and the state says only whether a handle was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeWait {
    /// A published, unresolved gate (`gate_waiting` with no
    /// `gate_resolved` after it). `external_ref` is the forge's handle
    /// (a PR URL) once one is recorded, `None` for an internal gate.
    Gate { external_ref: Option<String> },
    /// The questions the node asked and nobody answered
    /// (`questions_asked` with no `questions_answered` after it).
    Questions { asked: NonEmpty<QuestionId> },
}

impl NodeState {
    /// What this node waits on, for a reader that has a state and wants
    /// the wait without matching the whole enum.
    pub fn waiting_on(&self) -> Option<&NodeWait> {
        match self {
            NodeState::Waiting { on } => Some(on),
            _ => None,
        }
    }
}

/// A re-route as `node_rerouted` recorded it, on the node it left.
#[derive(Debug, Clone, PartialEq)]
pub struct Reroute {
    pub to: NodeId,
    pub cause: Failure,
    /// The retry count and the cap it counts against — both `None` for a
    /// gate's routing choice, which is a decision, not a retry.
    pub attempt: Option<u32>,
    pub max: Option<u32>,
    pub origin: RerouteOrigin,
    pub at: DateTime<Utc>,
    pub seq: Seq,
}

impl Reroute {
    /// The re-route `payload` records, placed by its envelope.
    pub fn of(payload: &NodeReroutedPayload, meta: &EventMeta<'_>) -> Self {
        Reroute {
            to: payload.to_node.clone(),
            cause: payload.cause.clone(),
            attempt: payload.attempt,
            max: payload.max_reroutes,
            origin: payload.origin,
            at: meta.at,
            seq: meta.seq,
        }
    }
}

/// The outcome of resolving one runner: the winning candidate plus every
/// candidate passed over, with reasons — exactly what `runner_resolved`
/// records.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRunner {
    pub runner: RunnerName,
    pub chosen: RunnerCandidate,
    pub discarded: Vec<DiscardedCandidate>,
}

/// What the log says about one node.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NodeRecord {
    /// The highest attempt number a `node_started` of this node carried. A
    /// retry is numbered after it, so this is the attempt a reader reports.
    pub attempts: u32,
    pub state: Option<NodeState>,
    /// Where and when the open attempt started; `None` once it closed.
    pub open_since: Option<(Seq, DateTime<Utc>)>,
    /// Where the latest attempt started; unlike `open_since`, kept once
    /// the attempt closes.
    pub last_started: Option<Seq>,
    /// When the node first started, across all of its attempts.
    pub first_started: Option<DateTime<Utc>>,
    /// The run's tree as the node's latest finish left it; `None` when
    /// it never finished, or its finish named no tree.
    pub left_tree: Option<TreeId>,
    /// The run's tree as the node's latest failure left it; `None` when
    /// it never failed, or its failure named no tree.
    pub failed_tree: Option<TreeId>,
    /// Whether another node moved the run's tree while this node's latest
    /// attempt ran: whatever that attempt verified, it may have read the
    /// tree half-changed. A new attempt starts without it.
    pub tree_moved: bool,
    /// The tree the open attempt started from — what its diff is judged
    /// against. `None` for an attempt that recorded none, which is what
    /// a log written before the audit had a starting point carries, and
    /// what a reader takes as the run's own base.
    pub from_tree: Option<TreeId>,
    /// What the open attempt of a node scoped to the run may change.
    pub run_scope: Option<Vec<crate::ScopeGlob>>,
    pub last_terminal: Option<Seq>,
    /// When the node last reached a terminal state.
    pub last_terminal_at: Option<DateTime<Utc>>,
    pub last_failed: Option<Seq>,
    pub last_finished: Option<Seq>,
    /// Every failure the node recorded, oldest first.
    pub failures: Vec<Failure>,
    pub reroutes: u32,
    /// The re-routes its own `on_failure` made, which its
    /// `max_reroutes` counts: a person's choice that sends the run
    /// somewhere is a decision, not a retry.
    pub on_failure_reroutes: u32,
    pub last_reroute: Option<Reroute>,
    pub runner: Option<ResolvedRunner>,
    /// The first runner the node resolved, with the seq of the event that
    /// resolved it.
    pub first_runner: Option<(Seq, ResolvedRunner)>,
    /// What the node spent: its settled work, and the usage its open
    /// attempt reported that nothing has settled.
    pub spend: Spend,
    /// What the open attempt has settled through its rounds of questions.
    /// A terminal folds it into the state it leaves and resets it.
    pub attempt_settled: TokenUsage,
    /// What the attempts after the first spent, settled.
    pub rework: TokenUsage,
    /// Where the open attempt's current working stretch began. `None` while
    /// the node waits on a person, and once the attempt has closed.
    pub working_since: Option<DateTime<Utc>>,
    /// The working time of every stretch that has closed, across attempts.
    pub active: Duration,
    /// The working time of the attempt now or last open. Reset when an
    /// attempt starts.
    pub attempt_active: Duration,
    /// The sessions the open attempt has opened, oldest first. A node's
    /// attempt is what bounds a session's life: the schema has no
    /// per-session close event, so a session is open exactly while the
    /// attempt that opened it has not reached a terminal.
    pub sessions: Vec<OpenSession>,
    /// The models this node's sessions ran on, as their CLIs reported them,
    /// each once, in the order the log first names it.
    pub models: Vec<crate::ids::ModelName>,
    /// Every write this node's sessions had refused, oldest first.
    pub refused: Vec<RefusedWrite>,
    /// Every stop this node's sessions were sent back from, oldest first.
    pub stops_refused: Vec<RefusedStop>,
    /// The tool calls the open attempt made, oldest first.
    pub calls: Vec<ToolCall>,
    /// Last failed run-tool call of the current or most recently closed
    /// attempt. A new start clears it; a terminal preserves it.
    pub last_tool_failure: Option<crate::events::RunToolFailedPayload>,
    pub last_event_at: Option<DateTime<Utc>>,
    /// Whether the node's session lost its service and has not got it back.
    /// A terminal clears it.
    pub unreachable: bool,
    /// The attempt before the open one, when it failed: its number and
    /// what it failed with. `None` when the open attempt follows no
    /// failure.
    pub failed_before: Option<FailedAttempt>,
    /// The session of a previous attempt that no terminal ever closed —
    /// what a resume finds when a crash cut the attempt between its
    /// start and its verdict. `None` once a later attempt closes
    /// normally, because then nothing was left open.
    pub orphaned_session: Option<OrphanedSession>,
    /// The last session this node opened of its own — not one a loop
    /// opened for a task — kept past the attempt's terminal: what a
    /// continuation after the answer to its scope request picks back up.
    pub last_session: Option<crate::ids::SessionId>,
}

/// A failed attempt as the next one remembers it.
#[derive(Debug, Clone, PartialEq)]
pub struct FailedAttempt {
    pub attempt: u32,
    pub failure: Failure,
}

impl NodeRecord {
    /// The attempt that just closed, when it failed — read before the
    /// next start replaces what it recorded.
    fn failed_attempt(&self) -> Option<FailedAttempt> {
        let Some(NodeState::Failed { failure, .. }) = &self.state else {
            return None;
        };
        Some(FailedAttempt {
            attempt: self.attempts,
            failure: failure.clone(),
        })
    }

    /// A `node_rerouted`: one more re-route, and the latest one kept.
    fn rerouted(&mut self, p: &NodeReroutedPayload, meta: &EventMeta<'_>) {
        self.reroutes += 1;
        if p.origin == RerouteOrigin::OnFailure {
            self.on_failure_reroutes += 1;
        }
        self.last_reroute = Some(Reroute::of(p, meta));
    }

    /// A `runner_resolved`: the runner the node runs on now, and the first
    /// one it ever resolved.
    fn resolved(&mut self, p: &RunnerResolvedPayload, meta: &EventMeta<'_>) {
        let resolved = ResolvedRunner {
            runner: p.runner.clone(),
            chosen: p.chosen.clone(),
            discarded: p.discarded.clone(),
        };
        if self.first_runner.is_none() {
            self.first_runner = Some((meta.seq, resolved.clone()));
        }
        self.runner = Some(resolved);
    }
}

/// Every node's record, by id.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NodeLedger {
    per_node: BTreeMap<NodeId, NodeRecord>,
    /// How many records have an attempt open or a session without its
    /// service: kept as each record changes, so a reader that asks on
    /// every event reads no record.
    tally: Tally,
}

impl NodeLedger {
    /// What the log says about `node`; `None` for a node it never names.
    pub fn get<Q>(&self, node: &Q) -> Option<&NodeRecord>
    where
        NodeId: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.per_node.get(node)
    }

    /// The tree `node`'s open attempt started from, as its own
    /// `node_started` recorded it — what its diff is judged against.
    /// `None` for a node that never started, and for one whose start
    /// named no tree.
    pub fn from_tree<Q>(&self, node: &Q) -> Option<&TreeId>
    where
        NodeId: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.per_node.get(node).and_then(|r| r.from_tree.as_ref())
    }

    /// What `node`'s open attempt, scoped to the run, may change, as its
    /// `node_started` recorded it.
    pub fn run_scope(&self, node: &NodeId) -> Option<&[crate::ScopeGlob]> {
        self.per_node.get(node).and_then(|r| r.run_scope.as_deref())
    }

    /// The newest tree the log records the run at, and where: every
    /// node's latest start, and the latest finish and failure of each
    /// node `counts_close` lets through. `None` when no event named a
    /// tree.
    pub fn latest_tree(&self, counts_close: impl Fn(&NodeId) -> bool) -> Option<(Seq, &TreeId)> {
        self.per_node
            .iter()
            .flat_map(|(id, record)| {
                let started = record.last_started.zip(record.from_tree.as_ref());
                let closes = counts_close(id);
                let finished = record
                    .last_finished
                    .zip(record.left_tree.as_ref())
                    .filter(|_| closes);
                let failed = record
                    .last_failed
                    .zip(record.failed_tree.as_ref())
                    .filter(|_| closes);
                started.into_iter().chain(finished).chain(failed)
            })
            .max_by_key(|(seq, _)| *seq)
    }

    /// Marks every attempt that ran beside `writer`'s — open while it
    /// ran, or closed after it started — when `writer` closed leaving the
    /// run's tree at `left`, different from the tree it started on. An
    /// attempt that itself started on `left` already saw the change.
    fn moved_beside(&mut self, writer: &NodeId, left: &TreeId) {
        let Some(record) = self.per_node.get(writer) else {
            return;
        };
        let (Some(from), Some(started)) = (&record.from_tree, record.last_started) else {
            return;
        };
        if from == left {
            return;
        }
        for (_, beside) in self.per_node.iter_mut().filter(|(id, _)| *id != writer) {
            let overlapped = matches!(beside.state, Some(NodeState::Running { .. }))
                || beside.last_terminal.is_some_and(|closed| closed > started);
            if overlapped && beside.from_tree.as_ref() != Some(left) {
                beside.tree_moved = true;
            }
        }
    }

    /// `node`'s derived state; `None` for a node with none yet.
    pub fn state<Q>(&self, node: &Q) -> Option<&NodeState>
    where
        NodeId: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.per_node.get(node).and_then(|r| r.state.as_ref())
    }

    /// Every node the log names, with its record, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (&NodeId, &NodeRecord)> {
        self.per_node.iter()
    }

    /// Every record, in node-id order — what a reader that asks about
    /// the run rather than one node walks.
    pub fn values(&self) -> impl Iterator<Item = &NodeRecord> {
        self.per_node.values()
    }

    /// Every record, in node-id order.
    pub fn records(&self) -> impl Iterator<Item = &NodeRecord> {
        self.per_node.values()
    }

    /// Whether the log names no node at all.
    pub fn is_empty(&self) -> bool {
        self.per_node.is_empty()
    }

    /// How many nodes the log names.
    pub fn len(&self) -> usize {
        self.per_node.len()
    }

    /// Whether `node` has a derived state: it started, closed, failed
    /// or is waiting. A node the log only mentions — a re-route target
    /// named by the node that left, a runner resolved ahead of time —
    /// has a record and no state, and has not run.
    pub fn has_state<Q>(&self, node: &Q) -> bool
    where
        NodeId: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.per_node
            .get(node)
            .is_some_and(|record| record.state.is_some())
    }

    /// Whether the log names `node` at all.
    pub fn contains<Q>(&self, node: &Q) -> bool
    where
        NodeId: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.per_node.contains_key(node)
    }

    /// Sets `node`'s state — for the pairs a node's own kinds do not
    /// close: a gate parks a node and its resolution restores it, and
    /// the gate ledger is what knows which.
    pub fn set_state(&mut self, node: &NodeId, state: Option<NodeState>) {
        self.per_node.entry(node.clone()).or_default().state = state;
    }

    /// Folds one node-domain event. `asleep` is the host's suspensions: a
    /// working stretch that ends here leaves out the time the host slept.
    pub fn apply(&mut self, event: &NodeEvent, meta: &EventMeta<'_>, asleep: &Suspensions) {
        let Some(node) = meta.node else {
            // A node event with no node on its envelope states nothing
            // about any node. The replay that dispatches here refuses it
            // before this point for the kinds where it is a broken log;
            // for the rest there is simply nothing to record.
            return;
        };
        // The id is copied only the first time the log names the node:
        // almost every event names a node the ledger holds.
        let record = match self.per_node.get_mut(node) {
            Some(record) => record,
            None => self.per_node.entry(node.clone()).or_default(),
        };
        let before = record.standing();
        record.last_event_at = Some(meta.at);
        match event {
            NodeEvent::Started(p) => record.started(p, meta),
            NodeEvent::Finished(p) => record.finished(p, meta, asleep),
            NodeEvent::Failed(p) => record.failed(p, meta, asleep),
            NodeEvent::Rerouted(p) => record.rerouted(p, meta),
            NodeEvent::RunnerResolved(p) => record.resolved(p, meta),
            // Audit: the engine ran a mechanical check around the node
            // and recorded what it found. None of it moves the node's
            // lifecycle — what a criterion or a scope diff decided
            // reaches state through the terminal the node then gets.
            NodeEvent::HookExecuted(_)
            | NodeEvent::ContextAssembled(_)
            | NodeEvent::CriteriaChecked(_)
            | NodeEvent::ScopeChecked(_)
            | NodeEvent::PullRequestOpened(_) => {}
        }
        let after = record.standing();
        self.track(before, after);
        if let Some(left) = left_by_close(event) {
            self.moved_beside(node, left);
        }
    }
}

/// The tree a node's close names: the one its finish or its failure
/// left the run at. `None` for any other event, and for a close that
/// named none.
fn left_by_close(event: &NodeEvent) -> Option<&TreeId> {
    match event {
        NodeEvent::Finished(p) => p.tree.as_ref(),
        NodeEvent::Failed(p) => p.tree.as_ref(),
        _ => None,
    }
}
