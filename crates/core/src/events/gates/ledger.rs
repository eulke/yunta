//! Every decision a person was asked for, folded once.
//!
//! Six places used to walk the gate kinds with their own rule — two of
//! them the same function copied byte for byte. Every surface that asks
//! what a node is waiting on, what was published, what was decided or
//! what was asked reads it here rather than walking those four kinds
//! itself: a second fold is a second answer.

use std::collections::{BTreeMap, BTreeSet};

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
    /// Where the log holds each resolution that answered a question about
    /// one of this node's tasks, a scope request or a departure. Such an
    /// answer is written on the node's own gate, but it is not the node's
    /// decision.
    pub task_answers: BTreeSet<Seq>,
    /// The forge's handle of the gate this node published and a wake
    /// polls: the latest, until a review asked for changes on it — the
    /// corrected lap publishes again.
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
    /// Whom a person is being asked about: the node, or `None` for a
    /// run-level ask. An asking opens it, and a resolution or an answer
    /// closes it.
    asking: BTreeSet<Option<NodeId>>,
    /// The nodes waiting on a question a person answers elsewhere: a round
    /// of questions, or a gate published to a forge.
    awaiting: BTreeSet<NodeId>,
}

impl GateLedger {
    /// What the log says about `node`'s gates.
    pub fn get(&self, node: &NodeId) -> Option<&GateRecord> {
        self.per_node.get(node)
    }

    /// Whom a person is being asked about right now: a node, or `None` for
    /// a run-level ask.
    pub fn asking(&self) -> &BTreeSet<Option<NodeId>> {
        &self.asking
    }

    /// The nodes waiting on an answer that a person gives elsewhere.
    pub fn awaiting(&self) -> &BTreeSet<NodeId> {
        &self.awaiting
    }

    /// Forgets every ask in progress: a run that pauses, resumes or closes
    /// is no longer asking anyone.
    pub fn clear_asking(&mut self) {
        self.asking.clear();
    }

    /// The forge's handle of the gate `node` published and a wake polls.
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
        self.track(event, meta);
        // An asking opening is held by the asking set alone: no node
        // record turns on it.
        if let GateEvent::AskingOpened(_) = event {
            return;
        }
        let Some(node) = meta.node else {
            // A run-level escalation: it gates the whole invocation, so
            // no node record answers for it.
            if let GateEvent::Waiting(p) = event {
                self.run_level.push((p.clone(), meta.seq));
            }
            return;
        };
        self.per_node
            .entry(node.clone())
            .or_default()
            .apply(event, meta.seq);
    }

    /// Keeps the asking and awaiting sets in step with one event.
    fn track(&mut self, event: &GateEvent, meta: &EventMeta<'_>) {
        match event {
            GateEvent::AskingOpened(_) => {
                self.asking.insert(meta.node.cloned());
            }
            GateEvent::QuestionsAsked(_) => self.awaiting.extend(meta.node.cloned()),
            GateEvent::Waiting(p) if p.external_ref().is_some() => {
                self.awaiting.extend(meta.node.cloned());
            }
            GateEvent::Waiting(_) => {}
            GateEvent::Resolved(_) | GateEvent::QuestionsAnswered(_) => {
                self.asking.remove(&meta.node.cloned());
                if let Some(node) = meta.node {
                    self.awaiting.remove(node);
                }
            }
        }
    }
}

impl GateRecord {
    /// The resolutions that are the node's own decisions, oldest first:
    /// every resolution except the answers to questions about its tasks.
    pub fn own_resolutions(&self) -> impl DoubleEndedIterator<Item = &(GateResolvedPayload, Seq)> {
        self.resolved
            .iter()
            .filter(move |(_, at)| !self.task_answers.contains(at))
    }

    /// Folds one gate event of this record's node, written at `seq`.
    fn apply(&mut self, event: &GateEvent, seq: Seq) {
        match event {
            GateEvent::Waiting(p) => {
                if let Some(reference) = p.external_ref() {
                    self.external_ref = Some(reference.to_string());
                }
                self.waiting = Some((p.clone(), seq));
            }
            GateEvent::Resolved(p) => {
                // A merge is an approval whose evidence is the merge
                // commit, and the payload already reads it as `Approved`.
                match p {
                    GateResolvedPayload::Approved { sha, .. } => {
                        self.approved_sha = Some(sha.clone());
                    }
                    // Changes requested end that round of review: what
                    // the correction makes is published, then reviewed.
                    GateResolvedPayload::ChangesRequested { .. } => self.external_ref = None,
                    GateResolvedPayload::Chosen(_)
                    | GateResolvedPayload::Closed
                    | GateResolvedPayload::Unrecognized(_) => {}
                }
                self.resolved.push((p.clone(), seq));
                // The escalation still standing is the question this answers.
                let answers_a_task = self
                    .waiting
                    .as_ref()
                    .is_some_and(|(asked, _)| asked.scope().is_some() || asked.departure.is_some());
                if answers_a_task {
                    self.task_answers.insert(seq);
                }
                self.waiting = None;
            }
            GateEvent::QuestionsAsked(p) => self.rounds.push(QuestionRound {
                asked: p.clone(),
                asked_at: seq,
                answered: None,
                answered_at: None,
            }),
            GateEvent::QuestionsAnswered(p) => {
                if let Some(round) = self.rounds.last_mut() {
                    round.answered = Some(p.clone());
                    round.answered_at = Some(seq);
                }
            }
            GateEvent::AskingOpened(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{
        DepartureMenu, Escalation, Evidence, Failure, FailureMenu, GateOption, HumanChoice,
        ScopeWait,
    };
    use crate::expansion::Waiting;
    use crate::reserved::ReservedOption;
    use crate::NonEmpty;

    fn option(choice: ReservedOption) -> GateOption {
        GateOption {
            id: choice.id(),
            label: None,
            tradeoff: None,
            asks: None,
        }
    }

    /// A question with two answers and nothing in its record.
    fn question(first: ReservedOption, second: ReservedOption) -> Escalation {
        Escalation::new(
            "a question with two answers",
            Evidence::none(),
            NonEmpty::from((option(first), vec![option(second)])),
        )
        .expect("a claim with no evidence repeats none")
    }

    fn apply_at(ledger: &mut GateLedger, node: &NodeId, seq: u64, event: GateEvent) {
        let meta = EventMeta {
            seq: Seq::from(seq),
            at: chrono::DateTime::UNIX_EPOCH,
            node: Some(node),
        };
        ledger.apply(&event, &meta);
    }

    fn chosen(choice: ReservedOption) -> GateEvent {
        GateEvent::Resolved(GateResolvedPayload::Chosen(HumanChoice {
            option: choice.id(),
            by: "lead".into(),
            free_text: None,
        }))
    }

    /// The event that writes `escalation` as the gate a node waits on.
    fn waiting(escalation: Escalation) -> GateEvent {
        GateEvent::Waiting(escalation.into_payload())
    }

    #[test]
    fn a_resolution_answering_a_scope_request_is_a_task_answer() {
        let node = NodeId::from("loop");
        let mut ledger = GateLedger::default();
        let scope = ScopeWait {
            because: Waiting::ModeAsk,
            proposed: None,
        };
        let escalation = question(ReservedOption::Grant, ReservedOption::Deny).with_scope(scope);
        apply_at(&mut ledger, &node, 1, waiting(escalation));
        apply_at(&mut ledger, &node, 2, chosen(ReservedOption::Deny));

        let record = ledger.get(&node).expect("the node has a gate record");
        assert_eq!(record.task_answers, BTreeSet::from([Seq::from(2)]));
        assert!(record.waiting.is_none());
    }

    #[test]
    fn a_resolution_answering_a_departure_is_a_task_answer() {
        let node = NodeId::from("loop");
        let mut ledger = GateLedger::default();
        let departure = DepartureMenu {
            deviations: Vec::new(),
            waived: Vec::new(),
            rewritten_by: None,
            accept_offered: true,
        };
        let escalation =
            question(ReservedOption::Accept, ReservedOption::Abort).with_departure(departure);
        apply_at(&mut ledger, &node, 1, waiting(escalation));
        apply_at(&mut ledger, &node, 2, chosen(ReservedOption::Accept));

        let record = ledger.get(&node).expect("the node has a gate record");
        assert_eq!(record.task_answers, BTreeSet::from([Seq::from(2)]));
    }

    #[test]
    fn a_resolution_of_a_failure_menu_is_the_nodes_own_decision() {
        let node = NodeId::from("loop");
        let mut ledger = GateLedger::default();
        let menu = FailureMenu {
            attempt: 1,
            failure: Failure::message("criteria still red"),
            remedies: Vec::new(),
            stuck: None,
            reroute: None,
        };
        let escalation = question(ReservedOption::Retry, ReservedOption::Abort).with_failure(menu);
        apply_at(&mut ledger, &node, 1, waiting(escalation));
        apply_at(&mut ledger, &node, 2, chosen(ReservedOption::Retry));

        let record = ledger.get(&node).expect("the node has a gate record");
        assert!(record.task_answers.is_empty());
        assert_eq!(record.resolved.len(), 1);
    }
}
