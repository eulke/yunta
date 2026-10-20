//! What is ready to run, and what it means when nothing is.

use yunta_core::events::PauseReason;
use yunta_core::{DefaultOnFailure, NodeId};

use super::{is_gate, Board, Decision};
use crate::replay::NodeState;

/// What is ready now, and what it means when nothing is: every node
/// finished closes the run, and anything else is stuck behind something
/// this run left unresolved.
pub(super) fn ready_batch(board: &Board<'_>) -> Decision {
    ready_gate(board)
        .or_else(|| ready_nodes(board))
        .unwrap_or_else(|| nothing_runnable(board))
}

/// A ready `kind: gate` is never batched with ordinary nodes — its
/// resolution is a forge round-trip, one at a time, same as every other
/// gate decision. A stateless ready gate is always unpublished
/// (external) or never-asked (internal); a published one carries
/// `Waiting` and [`waiting_step`](super::waiting_step) took it.
fn ready_gate(board: &Board<'_>) -> Option<Decision> {
    let node = board.nodes.iter().copied().find(|node| {
        is_gate(node)
            && !board.state.nodes.has_state(&node.id)
            && !board.reroute_only(&node.id)
            && board.deps_satisfied(node)
    })?;
    Some(board.drive_gate(node))
}

/// Fresh nodes whose dependencies are all finished, up to capacity, in
/// workflow declaration order.
fn ready_nodes(board: &Board<'_>) -> Option<Decision> {
    // A degenerate 0 would starve every ready node forever. `yunta
    // check` refuses it up front (`MaxParallelNodesZero`); this clamp
    // stays as defense in depth for a manifest frozen before that rule
    // existed — a stuck-looking run is worse than a sequential one.
    let capacity = board.policy.max_parallel_nodes.max(1) as usize;
    let batch: Vec<(NodeId, u32)> = board
        .nodes
        .iter()
        .copied()
        // finished, failed-and-handled above, a gate (handled above), or
        // a node only a re-route may start.
        .filter(|node| {
            !board.state.nodes.has_state(&node.id)
                && !is_gate(node)
                && !board.reroute_only(&node.id)
                && board.deps_satisfied(node)
        })
        .take(capacity)
        .map(|node| (node.id.clone(), 1))
        .collect();
    (!batch.is_empty()).then_some(Decision::Execute(batch))
}

/// Nothing is runnable: either every node this mode includes finished,
/// or something is stuck behind a failure this run left unresolved.
///
/// Only nodes the mode actually includes count toward completion — an
/// excluded node never reaches any terminal state (it is never scheduled
/// at all), so requiring it here would mean the run could never finish.
/// Same for an untouched reroute-only target that was simply never
/// needed: its source never failed, or the gate never chose its option.
fn nothing_runnable(board: &Board<'_>) -> Decision {
    let Some(stuck) = board.nodes.iter().copied().find(|node| {
        !board.finished(&node.id)
            && !(board.reroute_only(&node.id) && !board.state.nodes.has_state(&node.id))
    }) else {
        return Decision::Finish;
    };

    // Under `continue` a stuck run is the run's end — every node not
    // behind the failure has already run — so it closes failed, naming a
    // node that failed; under `pause` it freezes resumable.
    if board.policy.on_failure == DefaultOnFailure::Continue {
        let failed = board
            .nodes
            .iter()
            .find_map(|node| match board.state.nodes.state(&node.id) {
                Some(NodeState::Failed { failure, .. }) => Some((node.id.clone(), failure.clone())),
                _ => None,
            });
        if let Some((node, failure)) = failed {
            return Decision::Fail { node, failure };
        }
    }
    Decision::Pause {
        reason: PauseReason::Blocked {
            node: stuck.id.clone(),
            on: board
                .deps_of(&stuck.id)
                .iter()
                .filter(|dep| !board.finished(dep))
                .cloned()
                .collect(),
        },
    }
}
