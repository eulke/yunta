//! What sessions handed the run and what the engine answered: every
//! submission and finding call, counted once for the run and once for the
//! node each event's envelope names, and how many events each node has said.
//!
//! These counters answer "what was handed over", so they fold on their own
//! and share no accumulator with the spend or the clock.

use std::collections::BTreeMap;

use yunta_core::events::{
    ArtifactEvent, EventPayload, FindingEvent, SessionEvent, StoredEvent, SubmissionOutcome,
};
use yunta_core::rail::timing::Histogram;
use yunta_core::rail::RailEvent;
use yunta_core::NodeId;

/// Whole documents sessions offered, by what the engine answered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Submissions {
    pub accepted: u64,
    pub refused: u64,
}

/// Finding calls a log carries, by what the engine answered: the
/// accepted forms — a node's own finding posted, updated or withdrawn,
/// and another node's answered — and the calls it turned down.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct FindingActivity {
    pub posted: u64,
    pub updated: u64,
    pub withdrawn: u64,
    pub refused: u64,
    pub answered: u64,
    pub proved: u64,
}

/// The submissions and finding calls a log carries, for the run and for
/// each node that made one, and the rail's call timings merged per moment.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Activity {
    pub(crate) submissions: Submissions,
    pub(crate) submissions_by_node: BTreeMap<NodeId, Submissions>,
    pub(crate) findings: FindingActivity,
    pub(crate) findings_by_node: BTreeMap<NodeId, FindingActivity>,
    /// How long the rail's calls took at each moment, over every session's summary.
    pub(crate) rails: BTreeMap<RailEvent, Histogram>,
    /// How many events each node's envelope names, whatever they are: what a live
    /// view turns by, so a frame of it comes from the log and from no clock.
    pub(crate) events_by_node: BTreeMap<NodeId, u64>,
}

impl Activity {
    /// Counts `event` when it is a submission or a finding call, and merges
    /// a session's rail summary. Every other kind is nothing to count here.
    pub(crate) fn apply(&mut self, event: &StoredEvent) {
        let node = event.node_id.as_ref();
        if let Some(id) = node {
            // The id is copied only the first time the log names the node.
            match self.events_by_node.get_mut(id) {
                Some(count) => *count += 1,
                None => {
                    self.events_by_node.insert(id.clone(), 1);
                }
            }
        }
        match event.payload() {
            Some(EventPayload::Artifacts(ArtifactEvent::Submitted(p))) => count(
                &mut self.submissions,
                &mut self.submissions_by_node,
                node,
                match p.outcome {
                    SubmissionOutcome::Accepted { .. } => |s: &mut Submissions| &mut s.accepted,
                    SubmissionOutcome::Refused { .. } => |s: &mut Submissions| &mut s.refused,
                },
            ),
            Some(EventPayload::Findings(FindingEvent::Posted(_))) => {
                self.count_finding(node, |f| &mut f.posted)
            }
            Some(EventPayload::Findings(FindingEvent::Updated(_))) => {
                self.count_finding(node, |f| &mut f.updated)
            }
            Some(EventPayload::Findings(FindingEvent::Withdrawn(_))) => {
                self.count_finding(node, |f| &mut f.withdrawn)
            }
            Some(EventPayload::Findings(FindingEvent::Refused(_))) => {
                self.count_finding(node, |f| &mut f.refused)
            }
            Some(EventPayload::Findings(FindingEvent::Answered(_))) => {
                self.count_finding(node, |f| &mut f.answered)
            }
            Some(EventPayload::Findings(FindingEvent::Proved(_))) => {
                self.count_finding(node, |f| &mut f.proved)
            }
            Some(EventPayload::Session(SessionEvent::RailSummary(p))) => {
                for (moment, took) in &p.moments {
                    self.rails.entry(*moment).or_default().merge(took);
                }
            }
            _ => {}
        }
    }

    fn count_finding(
        &mut self,
        node: Option<&NodeId>,
        field: fn(&mut FindingActivity) -> &mut u64,
    ) {
        count(&mut self.findings, &mut self.findings_by_node, node, field);
    }
}

/// Adds one to the same counter of the run's own tally and of the node's,
/// so the two readings of one event can never differ. An envelope that
/// names no node counts for the run alone, and a node enters `by_node` on
/// its first event, never with a row of zeroes.
fn count<T: Default>(
    total: &mut T,
    by_node: &mut BTreeMap<NodeId, T>,
    node: Option<&NodeId>,
    field: fn(&mut T) -> &mut u64,
) {
    *field(total) += 1;
    if let Some(node) = node {
        *field(by_node.entry(node.clone()).or_default()) += 1;
    }
}
