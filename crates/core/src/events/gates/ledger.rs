//! Every decision a person was asked for, folded once.
//!
//! Six places used to walk the gate kinds with their own rule — two of
//! them the same function copied byte for byte. Every surface that asks
//! what a node is waiting on, what was published, what was decided or
//! what was asked reads it here rather than walking those four kinds
//! itself: a second fold is a second answer.

use std::collections::BTreeMap;

use crate::events::gates::kinds::GateEvent;
use crate::events::gates::payloads::{
    GateResolvedPayload, GateWaitingPayload, QuestionsAnsweredPayload, QuestionsAskedPayload,
};
use crate::events::meta::EventMeta;
use crate::hash::CommitSha;
use crate::ids::{NodeId, Seq};

/// One round of questions: what a node asked, and the answer it got.
#[derive(Debug, Clone, PartialEq)]
pub struct QuestionRound {
    pub asked: QuestionsAskedPayload,
    pub asked_at: Seq,
    /// The answer, once one was recorded. A round with none is the round
    /// the node is waiting on.
    pub answered: Option<QuestionsAnsweredPayload>,
    pub answered_at: Option<Seq>,
}

/// What the log says about one node's gates.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GateRecord {
    /// The escalation standing unresolved, and where it sits.
    pub waiting: Option<(GateWaitingPayload, Seq)>,
    /// Every resolution, oldest first.
    pub resolved: Vec<(GateResolvedPayload, Seq)>,
    /// The forge's handle of the latest gate this node published.
    pub external_ref: Option<String>,
    /// The commit the latest approving review covered.
    pub approved_sha: Option<CommitSha>,
    /// Every round of questions this node asked, oldest first.
    pub rounds: Vec<QuestionRound>,
}

/// Every node's gates, by id.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GateLedger {
    per_node: BTreeMap<NodeId, GateRecord>,
    /// Run-level escalations: the token budget gates the whole
    /// invocation, not any node.
    run_level: Vec<(GateWaitingPayload, Seq)>,
}

impl GateLedger {
    /// What the log says about `node`'s gates.
    pub fn get(&self, node: &NodeId) -> Option<&GateRecord> {
        self.per_node.get(node)
    }

    /// The forge's handle of the latest gate `node` published.
    pub fn last_external_ref(&self, node: &NodeId) -> Option<&str> {
        self.per_node
            .get(node)
            .and_then(|record| record.external_ref.as_deref())
    }

    /// The commit the latest approving review of `node`'s gate covered.
    pub fn approved_sha(&self, node: &NodeId) -> Option<&CommitSha> {
        self.per_node
            .get(node)
            .and_then(|record| record.approved_sha.as_ref())
    }

    /// A resolution recorded while the run was parked, with no
    /// escalation standing after it: what a `resolve_gate` call seeded
    /// onto the log for this wake to consume.
    pub fn pre_seeded(&self, node: &NodeId) -> Option<&GateResolvedPayload> {
        let record = self.per_node.get(node)?;
        let (resolution, at) = record.resolved.last()?;
        let waiting_after = record.waiting.as_ref().is_some_and(|(_, seq)| seq > at);
        (!waiting_after).then_some(resolution)
    }

    /// Whether `node`'s latest round was answered. Its close already ran
    /// when it asked, so what it owes is a terminal, and the scheduler
    /// pays that from the log rather than running a session again.
    pub fn answered(&self, node: &NodeId) -> bool {
        self.per_node
            .get(node)
            .and_then(|record| record.rounds.last())
            .is_some_and(|round| round.answered.is_some())
    }

    /// Every run-level escalation, oldest first.
    pub fn run_level(&self) -> &[(GateWaitingPayload, Seq)] {
        &self.run_level
    }

    /// Folds one gate-domain event.
    pub fn apply(&mut self, event: &GateEvent, meta: &EventMeta<'_>) {
        let Some(node) = meta.node else {
            // A run-level escalation: it gates the whole invocation, so
            // no node record answers for it.
            if let GateEvent::Waiting(p) = event {
                self.run_level.push((p.clone(), meta.seq));
            }
            return;
        };
        let record = self.per_node.entry(node.clone()).or_default();
        match event {
            GateEvent::Waiting(p) => {
                if let Some(reference) = p.external_ref() {
                    record.external_ref = Some(reference.to_string());
                }
                record.waiting = Some((p.clone(), meta.seq));
            }
            GateEvent::Resolved(p) => {
                // A merge is an approval whose evidence is the merge
                // commit, and the payload already reads it as `Approved`.
                if let GateResolvedPayload::Approved { sha, .. } = p {
                    record.approved_sha = Some(sha.clone());
                }
                record.resolved.push((p.clone(), meta.seq));
                record.waiting = None;
            }
            GateEvent::QuestionsAsked(p) => record.rounds.push(QuestionRound {
                asked: p.clone(),
                asked_at: meta.seq,
                answered: None,
                answered_at: None,
            }),
            GateEvent::QuestionsAnswered(p) => {
                if let Some(round) = record.rounds.last_mut() {
                    round.answered = Some(p.clone());
                    round.answered_at = Some(meta.seq);
                }
            }
        }
    }
}
