//! What a person decided about a node, read off the run's state: the
//! resolution a parked run was handed, the choice made after a failure,
//! and whether that choice runs the node again.

use yunta_core::events::{GateResolvedPayload, HumanChoice};
use yunta_core::{NodeId, Seq};

use super::RunState;

impl RunState {
    /// A resolution recorded while the run was parked and nothing has
    /// consumed: what a `resolve_gate` call seeded onto the log for this
    /// wake to act on.
    ///
    /// Three ledgers answer this together, which is why it lives here.
    /// The gate ledger holds the resolution and whether an escalation
    /// stands after it; the node ledger holds the terminals and
    /// re-routes that mean the run already acted on it; the run ledger
    /// holds the pause that ends the window it was seeded into. A
    /// decision is pre-seeded only while none of those came after it.
    pub fn pre_seeded(&self, node: &NodeId) -> Option<&GateResolvedPayload> {
        let record = self.gates.get(node)?;
        let (resolution, at) = record.resolved.last()?;
        let consumed = self.nodes.get(node).into_iter().flat_map(|node| {
            [
                node.last_terminal,
                node.last_reroute.as_ref().map(|r| r.seq),
            ]
        });
        let blocker = consumed.flatten().chain(self.run.last_paused_at()).max();
        let standing = record.waiting.as_ref().map(|(_, seq)| *seq);
        (Some(*at) > blocker && Some(*at) > standing).then_some(resolution)
    }

    /// Whether a person chose to run `node` again after its latest
    /// failure, a choice nothing has acted on: the decision's window is
    /// [`Self::pre_seeded`]'s, and the attempt it starts closes it, so it
    /// runs the node once.
    pub fn run_again_chosen(&self, node: &NodeId) -> bool {
        matches!(
            self.pre_seeded(node),
            Some(GateResolvedPayload::Chosen(choice))
                if crate::reserved::ReservedOption::of(&choice.option)
                    .is_some_and(crate::reserved::ReservedOption::runs_again)
        )
    }

    /// The choice a person made about `node` after its latest failure,
    /// and where the log holds it. `None` when the node never failed, or
    /// its latest decision came before that failure or chose nothing.
    ///
    /// Whether anything already acted on the choice is the caller's
    /// question: what it does with it — reopen tasks, widen a scope —
    /// leaves its own mark on the log.
    pub fn choice_after_failure(&self, node: &NodeId) -> Option<(Seq, &HumanChoice)> {
        let failed = self.nodes.get(node)?.last_failed?;
        let (resolution, at) = self.gates.get(node)?.resolved.last()?;
        let GateResolvedPayload::Chosen(choice) = resolution else {
            return None;
        };
        (*at > failed).then_some((*at, choice))
    }
}
