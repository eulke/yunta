//! An invariant's pass holds for the tree it left.
//!
//! A `bash` or tree-judging `check` marked `invariant` verified the run's
//! tree as it stood when it finished. When a later node changes that tree
//! — a corrective node, a node after the checks, a person between two
//! attempts — the pass speaks for a tree the run no longer holds, so the
//! invariant runs again before anything else starts, a gate is asked, or
//! the run finishes. It costs no session: the input it depends on changed.

use yunta_core::Node;

use super::{Board, Decision};
use crate::replay::NodeState;

/// The first invariant, in declaration order, whose pass no longer speaks
/// for the run's tree — run again, alone, once nothing else is running.
///
/// The run's tree is the newest one the log records: any node's start, or
/// the finish of a node that is not itself such an invariant. An
/// invariant's own finish is left out, so one that rewrites files never
/// makes another stale and two never send each other round; what it
/// rewrote is seen at the next node that touches the tree. A pass whose
/// finish named no tree — a log written before finishes named one —
/// is never taken for stale.
pub(super) fn reverify_step(board: &Board<'_>) -> Option<Decision> {
    let running = board
        .state
        .nodes
        .values()
        .any(|record| matches!(record.state, Some(NodeState::Running { .. })));
    if running {
        return None;
    }
    let verifiers: Vec<&Node> = board
        .nodes
        .iter()
        .copied()
        .filter(|node| node.verifies_the_tree())
        .collect();
    let (at, now) = board
        .state
        .nodes
        .latest_tree(|id| !verifiers.iter().any(|node| &node.id == id))?;
    let stale = verifiers.into_iter().find(|node| {
        board.state.nodes.get(&node.id).is_some_and(|record| {
            matches!(record.state, Some(NodeState::Finished { .. }))
                && record.last_finished.is_some_and(|finished| finished < at)
                && record.left_tree.as_ref().is_some_and(|left| left != now)
        })
    })?;
    Some(Decision::Execute(vec![(
        stale.id.clone(),
        board.next_attempt(&stale.id),
    )]))
}
