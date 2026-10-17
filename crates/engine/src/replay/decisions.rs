//! What a person decided about a node, read off the run's state: the
//! resolution a parked run was handed, the choice made after a failure,
//! whether that choice runs the node again, and whether a wait for a limit's
//! reset is over.

use chrono::{DateTime, Utc};
use yunta_core::events::{GateResolvedPayload, HumanChoice, PauseReason};
use yunta_core::reserved::ReservedOption;
use yunta_core::{NodeId, Seq};

use super::RunState;

impl RunState {
    /// A resolution recorded while the run was parked and nothing has
    /// consumed: what a `resolve_gate` call seeded onto the log for this
    /// wake to act on, and where the log holds it. Every resolution counts,
    /// a task's answer included, since the question it answers is what acts
    /// on it.
    pub fn pre_seeded(&self, node: &NodeId) -> Option<(&GateResolvedPayload, Seq)> {
        let record = self.gates.get(node)?;
        let (resolution, at) = record.resolved.last()?;
        self.unconsumed(node, *at).then_some((resolution, *at))
    }

    /// The option a person chose to run `node` again after its latest
    /// failure, while nothing has acted on that choice. Only the node's own
    /// resolutions count: an answer to a question about one of its tasks is
    /// never the choice made about the failure, so it never runs the node
    /// again. The window is the one [`Self::pre_seeded`] reads, and the
    /// attempt it starts closes it, so the choice runs the node once.
    pub fn run_again_chosen(&self, node: &NodeId) -> Option<ReservedOption> {
        let record = self.gates.get(node)?;
        let (resolution, at) = record.own_resolutions().next_back()?;
        let GateResolvedPayload::Chosen(choice) = resolution else {
            return None;
        };
        let option = ReservedOption::of(&choice.option)?;
        (option.runs_again() && self.unconsumed(node, *at)).then_some(option)
    }

    /// The choice a person made about `node` after its latest failure,
    /// and where the log holds it. `None` when the node never failed, or
    /// its latest decision came before that failure or chose nothing.
    ///
    /// A decision is the node's latest resolution that did not answer a
    /// question about one of its tasks: a scope request or a departure is
    /// answered on the node's own gate, and such an answer is never the
    /// choice made about a failure, however late it comes.
    ///
    /// Whether anything already acted on the choice is the caller's
    /// question: what it does with it — reopen tasks, widen a scope —
    /// leaves its own mark on the log.
    pub fn choice_after_failure(&self, node: &NodeId) -> Option<(Seq, &HumanChoice)> {
        let failed = self.nodes.get(node)?.last_failed?;
        let record = self.gates.get(node)?;
        let (resolution, at) = record.own_resolutions().next_back()?;
        let GateResolvedPayload::Chosen(choice) = resolution else {
            return None;
        };
        (*at > failed).then_some((*at, choice))
    }

    /// Whether the resolution at `at` on `node`'s gate has nothing after it
    /// that consumed it. Three ledgers answer this together, which is why it
    /// lives here. The gate ledger holds the resolution and whether an
    /// escalation stands after it; the node ledger holds the terminals and
    /// re-routes that mean the run already acted on it; the run ledger holds
    /// the pause that ends the window the resolution was seeded into. A
    /// decision is unconsumed only while none of those came after it. An
    /// abort is one of those decisions: the pause it leads to ends its
    /// window, so the question comes back on the next resume.
    fn unconsumed(&self, node: &NodeId, at: Seq) -> bool {
        let standing = self
            .gates
            .get(node)
            .and_then(|record| record.waiting.as_ref())
            .map(|(_, seq)| *seq);
        let consumed = self.nodes.get(node).into_iter().flat_map(|node| {
            [
                node.last_terminal,
                node.last_reroute.as_ref().map(|r| r.seq),
            ]
        });
        let blocker = consumed.flatten().chain(self.run.last_paused_at()).max();
        Some(at) > blocker && Some(at) > standing
    }

    /// Whether the run paused awaiting `node`'s limit to reset at `at`, and
    /// a wake logged at or after that reset has resumed it. The engine never
    /// waits for a reset: a wake before it pauses again on the same reset, so
    /// only a resume from the reset on ends the wait.
    pub fn reset_waited(&self, node: &NodeId, at: DateTime<Utc>) -> bool {
        matches!(
            self.run.last_pause(),
            Some((PauseReason::AwaitingReset { node: waited, at: due }, _))
                if waited == node && *due == at
        ) && self.run.paused().is_none()
            && self.run.resumed_at().is_some_and(|woke| woke >= at)
    }
}
