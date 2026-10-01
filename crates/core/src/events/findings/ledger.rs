//! The one fold from finding events to the set a run actually holds.
//!
//! A finding arrives on its own and can be replaced or taken back, so
//! what stands now is not what the log says was posted: it is the last
//! state of every id nobody withdrew. Every surface that counts,
//! renders, inherits or writes findings reads that set from here rather
//! than folding `finding_posted` itself — with three events per finding,
//! a second fold is a second answer.
//!
//! Ownership is by node, not by session or by run: the id a node posts
//! is unique among that node's own findings, and only that node's later
//! events reach it. Two nodes may each hold a finding called `dup-1`.
//! The engine posts findings of its own about the run rather than about
//! a node — a cleanup that could not finish, an artifact a distill did
//! not find — and those carry no node. They stand like any other and
//! are counted like any other; what they have no owner for is being
//! updated or withdrawn, which nothing does to them.
//!
//! Another node may answer a finding — its work fixed it, or it declines
//! to — and the answer stands beside the finding it answers, never in
//! it: an answer is what a node says, and the finding is still what was
//! found. An answer lasts while what it answered does: an update replaces
//! the finding it was about, and a withdrawal takes it away.

use std::collections::BTreeMap;

use crate::events::FindingEvent;
use crate::events::{EventPayload, Finding, FindingAnswer, StoredEvent};
use crate::ids::{FindingId, NodeId};

/// A finding as the run holds it now, and which node holds it — `None`
/// for one the engine posted about the run itself.
#[derive(Debug, Clone, PartialEq)]
pub struct PostedFinding {
    pub node: Option<NodeId>,
    pub finding: Finding,
}

/// What one node answered about a finding: the node, `None` for an
/// answer the log carries without one, what it answered and why.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnswerGiven {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<NodeId>,
    pub answer: FindingAnswer,
    pub why: String,
}

/// Where one id stands.
#[derive(Debug, Clone, PartialEq)]
pub enum Slot {
    /// Posted, possibly replaced since, and not withdrawn.
    Live(Finding),
    /// Taken back, with the reason its node gave. Final.
    Withdrawn { reason: String },
}

/// Every finding a log has touched, by the node that posted it.
///
/// Folds in log order and answers two questions: where one id stands —
/// what the run tools check before accepting a post, an update or a
/// withdrawal — and which findings stand now.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FindingLedger {
    slots: BTreeMap<(Option<NodeId>, FindingId), Slot>,
    /// The order ids were first posted in, which is the order the
    /// effective set reads in: an update moves nothing, so a reviewer
    /// sees their findings where they left them.
    first_post: Vec<(Option<NodeId>, FindingId)>,
    /// What other nodes answered about each finding that stands, one
    /// answer per answering node, in the order they first answered.
    answers: BTreeMap<(Option<NodeId>, FindingId), Vec<AnswerGiven>>,
}

impl FindingLedger {
    /// Folds every event of `events`, in order.
    pub fn of<'a>(events: impl IntoIterator<Item = &'a StoredEvent>) -> Self {
        let mut ledger = FindingLedger::default();
        for event in events {
            if let Some(EventPayload::Findings(e)) = event.payload() {
                ledger.apply(event.node_id.as_ref(), e);
            }
        }
        ledger
    }

    /// Applies one event.
    ///
    /// Total, and ignores what it cannot use: an event about something
    /// else, and a sequence the engine never writes — an update or a
    /// withdrawal for an id its node never posted, or a post on an id it
    /// withdrew. The run tools refuse those before they are appended, so
    /// meeting one here means the log came from somewhere else, and the
    /// honest reading of it is the state it can account for rather than
    /// a panic.
    pub fn apply(&mut self, node: Option<&NodeId>, event: &FindingEvent) {
        let node = node.cloned();
        match event {
            FindingEvent::Posted(p) => {
                let key = (node, p.finding.id.clone());
                match self.slots.get(&key) {
                    Some(Slot::Withdrawn { .. }) | Some(Slot::Live(_)) => {}
                    None => {
                        self.first_post.push(key.clone());
                        self.slots.insert(key, Slot::Live(p.finding.clone()));
                    }
                }
            }
            FindingEvent::Updated(p) => {
                let key = (node, p.finding.id.clone());
                if matches!(self.slots.get(&key), Some(Slot::Live(_))) {
                    self.answers.remove(&key);
                    self.slots.insert(key, Slot::Live(p.finding.clone()));
                }
            }
            FindingEvent::Withdrawn(p) => {
                let key = (node, p.id.clone());
                if matches!(self.slots.get(&key), Some(Slot::Live(_))) {
                    self.answers.remove(&key);
                    self.slots.insert(
                        key,
                        Slot::Withdrawn {
                            reason: p.reason.clone(),
                        },
                    );
                }
            }
            // The engine refused an operation: what was wrong with it
            // is in the event and nowhere else, and nothing a run holds
            // changed.
            FindingEvent::Refused(_) => {}
            FindingEvent::Answered(p) => self.answered(node, p),
        }
    }

    /// Folds one answer: kept beside the finding it answers while that
    /// finding stands, and replacing what the same node answered before.
    fn answered(&mut self, by: Option<NodeId>, answer: &crate::events::FindingAnsweredPayload) {
        let key = (Some(answer.node.clone()), answer.id.clone());
        if !matches!(self.slots.get(&key), Some(Slot::Live(_))) {
            return;
        }
        let given = AnswerGiven {
            by,
            answer: answer.answer,
            why: answer.why.clone(),
        };
        let answers = self.answers.entry(key).or_default();
        match answers.iter_mut().find(|earlier| earlier.by == given.by) {
            Some(earlier) => *earlier = given,
            None => answers.push(given),
        }
    }

    /// What other nodes answered about the finding `id` that `node`
    /// reported, while it stands.
    pub fn answers(&self, node: Option<&NodeId>, id: &FindingId) -> &[AnswerGiven] {
        self.answers
            .get(&(node.cloned(), id.clone()))
            .map_or(&[], Vec::as_slice)
    }

    /// Where `id` stands for `node`, or `None` if that node never posted
    /// it. Asked by the run tools, which speak for a node, so a session
    /// never reaches a finding the engine posted about the run.
    pub fn status(&self, node: &NodeId, id: &FindingId) -> Option<&Slot> {
        self.slots.get(&(Some(node.clone()), id.clone()))
    }

    /// Every finding that stands, in the order each was first posted.
    pub fn effective(&self) -> Vec<PostedFinding> {
        self.first_post
            .iter()
            .filter_map(|key| match self.slots.get(key) {
                Some(Slot::Live(finding)) => Some(PostedFinding {
                    node: key.0.clone(),
                    finding: finding.clone(),
                }),
                _ => None,
            })
            .collect()
    }

    /// Every finding that stands, as one view: each with the node that
    /// reported it and how other nodes answered it, in the order each
    /// was first posted.
    pub fn standing(&self) -> super::standing::RunFindings {
        super::standing::RunFindings {
            findings: self
                .effective()
                .into_iter()
                .map(|posted| super::standing::StandingFinding {
                    answers: self
                        .answers(posted.node.as_ref(), &posted.finding.id)
                        .to_vec(),
                    node: posted.node,
                    finding: posted.finding,
                })
                .collect(),
        }
    }

    /// The same, for one node — never the engine's own run-level
    /// findings, which belong to no node's artifact.
    pub fn effective_of(&self, node: &NodeId) -> Vec<Finding> {
        self.first_post
            .iter()
            .filter(|(posted_by, _)| posted_by.as_ref() == Some(node))
            .filter_map(|key| match self.slots.get(key) {
                Some(Slot::Live(finding)) => Some(finding.clone()),
                _ => None,
            })
            .collect()
    }
}

/// Every finding `events` leaves standing. [`FindingLedger::of`] when a
/// caller also needs to ask about one id.
pub fn effective<'a>(events: impl IntoIterator<Item = &'a StoredEvent>) -> Vec<PostedFinding> {
    FindingLedger::of(events).effective()
}
