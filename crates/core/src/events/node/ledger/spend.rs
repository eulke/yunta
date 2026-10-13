//! A node's attempts as their spend and working time run: what a start
//! begins with, what a round of questions settles, and what a terminal
//! folds into the record and the state it leaves.

use std::time::Duration;

use chrono::{DateTime, Utc};

use super::{NodeLedger, NodeRecord, NodeState, OrphanedSession};
use crate::events::meta::EventMeta;
use crate::events::node::payloads::{NodeFailedPayload, NodeFinishedPayload, NodeStartedPayload};
use crate::events::{Suspensions, TokenUsage};
use crate::ids::NodeId;

impl NodeRecord {
    /// A node's `node_started`: the attempt opens from the tree the start
    /// recorded, working, with nothing settled or in flight.
    pub(super) fn started(&mut self, p: &NodeStartedPayload, meta: &EventMeta<'_>) {
        self.failed_before = self.failed_attempt();
        self.from_tree = p.from_tree.clone();
        self.run_scope = p.run_scope.clone();
        // A start while the previous attempt is still open means nothing
        // closed it: whatever session it had is the orphan a resume has to
        // deal with.
        self.orphaned_session = self
            .open_since
            .is_some()
            .then(|| match self.sessions.last() {
                Some(session) => OrphanedSession::Open(session.session_id.clone()),
                None => OrphanedSession::NoneRecorded,
            });
        self.attempts = self.attempts.max(p.attempt);
        self.state = Some(NodeState::Running { attempt: p.attempt });
        self.open_since = Some((meta.seq, meta.at));
        self.last_started = Some(meta.seq);
        self.first_started = self.first_started.or(Some(meta.at));
        self.tree_moved = false;
        self.sessions.clear();
        self.calls.clear();
        self.last_tool_failure = None;
        self.working_since = Some(meta.at);
        self.attempt_active = Duration::ZERO;
        self.attempt_settled = TokenUsage::default();
        self.spend.in_flight = TokenUsage::default();
    }

    /// A node's `node_finished`: the attempt closes with its outcome, and
    /// the state it leaves carries everything the attempt spent.
    pub(super) fn finished(
        &mut self,
        p: &NodeFinishedPayload,
        meta: &EventMeta<'_>,
        asleep: &Suspensions,
    ) {
        let attempt = self.end_attempt(p.tokens_used, meta, asleep);
        self.last_finished = Some(meta.seq);
        self.left_tree = p.tree.clone();
        self.state = Some(NodeState::Finished {
            outcome: p.outcome.clone(),
            tokens: attempt,
        });
    }

    /// A node's `node_failed`: the attempt closes with its failure, which
    /// joins the failures the record already keeps.
    pub(super) fn failed(
        &mut self,
        p: &NodeFailedPayload,
        meta: &EventMeta<'_>,
        asleep: &Suspensions,
    ) {
        let attempt = self.end_attempt(p.tokens_used, meta, asleep);
        self.last_failed = Some(meta.seq);
        self.failed_tree = p.tree.clone();
        self.failures.push(p.failure.clone());
        self.state = Some(NodeState::Failed {
            failure: p.failure.clone(),
            tokens: attempt,
        });
    }

    /// Closes the open attempt at a terminal that spent `tokens`: the spend
    /// settles, the working stretch ends and the attempt's sessions and calls
    /// are over. Returns what the attempt spent in all, which its state keeps.
    fn end_attempt(
        &mut self,
        tokens: TokenUsage,
        meta: &EventMeta<'_>,
        asleep: &Suspensions,
    ) -> TokenUsage {
        let attempt = self.attempt_settled + tokens;
        self.spend.settled += tokens;
        if self.attempts > 1 {
            self.rework += tokens;
        }
        self.spend.in_flight = TokenUsage::default();
        self.attempt_settled = TokenUsage::default();
        self.close_working(meta.at, asleep);
        self.open_since = None;
        self.last_terminal = Some(meta.seq);
        self.last_terminal_at = Some(meta.at);
        self.unreachable = false;
        self.sessions.clear();
        self.calls.clear();
        attempt
    }

    /// Ends the open working stretch at `at`: its awake time counts toward
    /// the node's working time and toward this attempt's. A node that is not
    /// working, because it waits on a person or no attempt is open, has no
    /// stretch to end.
    fn close_working(&mut self, at: DateTime<Utc>, asleep: &Suspensions) {
        if let Some(since) = self.working_since.take() {
            let stretch = asleep.awake_between(since, at);
            self.active += stretch;
            self.attempt_active += stretch;
        }
    }
}

impl NodeLedger {
    /// A round of questions closes: the usage its session reported moves from
    /// in flight to settled, and the node stops working while a person
    /// answers. The attempt may have other sessions reporting usage, so only
    /// what this session reported leaves in flight.
    pub fn settle_asking(
        &mut self,
        node: &NodeId,
        asked: TokenUsage,
        meta: &EventMeta<'_>,
        asleep: &Suspensions,
    ) {
        let record = self.per_node.entry(node.clone()).or_default();
        record.spend.settled += asked;
        record.spend.in_flight = record.spend.in_flight.saturating_sub(asked);
        record.attempt_settled += asked;
        if record.attempts > 1 {
            record.rework += asked;
        }
        self.stop_working(node, meta, asleep);
    }

    /// The node stops working: a person is being asked, so its open working
    /// stretch ends at the time of `meta`.
    pub fn stop_working(&mut self, node: &NodeId, meta: &EventMeta<'_>, asleep: &Suspensions) {
        if let Some(record) = self.per_node.get_mut(node) {
            record.close_working(meta.at, asleep);
        }
    }

    /// The answer or the resolution lands and the node works again from
    /// `at`. A node that is already working keeps the start of its stretch.
    pub fn resume_working(&mut self, node: &NodeId, at: DateTime<Utc>) {
        if let Some(record) = self.per_node.get_mut(node) {
            if record.open_since.is_some() {
                record.working_since = record.working_since.or(Some(at));
            }
        }
    }
}
