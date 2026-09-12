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

use std::collections::BTreeMap;

use super::{EventPayload, Finding, StoredEvent};
use crate::ids::{FindingId, NodeId};

/// A finding as the run holds it now, and which node holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct PostedFinding {
    pub node: NodeId,
    pub finding: Finding,
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
    slots: BTreeMap<(NodeId, FindingId), Slot>,
    /// The order ids were first posted in, which is the order the
    /// effective set reads in: an update moves nothing, so a reviewer
    /// sees their findings where they left them.
    first_post: Vec<(NodeId, FindingId)>,
}

impl FindingLedger {
    /// Folds every event of `events`, in order.
    pub fn of<'a>(events: impl IntoIterator<Item = &'a StoredEvent>) -> Self {
        let mut ledger = FindingLedger::default();
        for event in events {
            if let Some(payload) = event.payload() {
                ledger.apply(event.node_id.as_ref(), payload);
            }
        }
        ledger
    }

    /// Applies one event.
    ///
    /// Total, and ignores what it cannot use: an event about something
    /// else, an event with no node, and a sequence the engine never
    /// writes — an update or a withdrawal for an id its node never
    /// posted, or a post on an id it withdrew. The run tools refuse
    /// those before they are appended, so meeting one here means the log
    /// came from somewhere else, and the honest reading of it is the
    /// state it can account for rather than a panic.
    pub fn apply(&mut self, node: Option<&NodeId>, payload: &EventPayload) {
        let Some(node) = node else { return };
        match payload {
            EventPayload::FindingPosted(p) => {
                let key = (node.clone(), p.finding.id.clone());
                match self.slots.get(&key) {
                    Some(Slot::Withdrawn { .. }) | Some(Slot::Live(_)) => {}
                    None => {
                        self.first_post.push(key.clone());
                        self.slots.insert(key, Slot::Live(p.finding.clone()));
                    }
                }
            }
            EventPayload::FindingUpdated(p) => {
                let key = (node.clone(), p.finding.id.clone());
                if matches!(self.slots.get(&key), Some(Slot::Live(_))) {
                    self.slots.insert(key, Slot::Live(p.finding.clone()));
                }
            }
            EventPayload::FindingWithdrawn(p) => {
                let key = (node.clone(), p.id.clone());
                if matches!(self.slots.get(&key), Some(Slot::Live(_))) {
                    self.slots.insert(
                        key,
                        Slot::Withdrawn {
                            reason: p.reason.clone(),
                        },
                    );
                }
            }
            _ => {}
        }
    }

    /// Where `id` stands for `node`, or `None` if that node never posted
    /// it.
    pub fn status(&self, node: &NodeId, id: &FindingId) -> Option<&Slot> {
        self.slots.get(&(node.clone(), id.clone()))
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

    /// The same, for one node.
    pub fn effective_of(&self, node: &NodeId) -> Vec<Finding> {
        self.first_post
            .iter()
            .filter(|(posted_by, _)| posted_by == node)
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
