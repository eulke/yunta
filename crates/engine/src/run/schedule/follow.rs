//! When a run moves into the checkout its last unit built in. A unit's
//! checkout holds a build of exactly the commit its work landed at, and
//! the run's own tree, which nothing built in since, would build the same
//! commit again from cold for the next node that works there. So before
//! such a node, a run whose units' work landed since anything last built
//! in its tree takes that unit's checkout — warm — as its tree.

use yunta_core::events::NodeState;
use yunta_core::{Isolation, Node, NodeId, NodeKind, Workflow};

use super::Policy;
use crate::replay::RunState;

/// Whether the batch `batch` is the moment for the run to look for the
/// checkout its last unit built in: the run works in a checkout of its
/// own, nothing else is under way, a node of the batch builds in the
/// run's tree, and units' work landed since the run took that checkout
/// and since a node last started building there.
pub fn follows_build(
    workflow: &Workflow,
    state: &RunState,
    policy: &Policy,
    batch: &[(NodeId, u32)],
) -> bool {
    let busy = state.nodes.records().any(|record| {
        matches!(
            record.state,
            Some(NodeState::Running { .. } | NodeState::Waiting { .. })
        )
    });
    let builds_here = batch
        .iter()
        .filter_map(|(id, _)| workflow.iter_nodes().find(|node| &node.id == id))
        .any(builds_in_the_runs_tree);
    policy.follows_builds && !busy && builds_here && landed_since_built(workflow, state)
}

/// Whether running `node` builds in the run's own tree: a command or a
/// session there, or a child run sharing it — not one in a checkout of
/// its own.
fn builds_in_the_runs_tree(node: &Node) -> bool {
    let here = match &node.kind {
        NodeKind::Bash { .. } | NodeKind::Prompt { .. } | NodeKind::Executor { .. } => true,
        NodeKind::Check(builtin) => builtin.judges_the_tree(),
        NodeKind::Workflow { isolation, .. } => *isolation == Isolation::None,
        _ => false,
    };
    here && !crate::scope::audits(node)
}

/// Whether units' work — a loop's tasks', or an audited node's — last
/// landed after the run took its checkout, and after a node last started
/// building in it.
fn landed_since_built(workflow: &Workflow, state: &RunState) -> bool {
    let Some(bound) = state.run.bound_at() else {
        return false;
    };
    let (mut landed, mut built) = (None, None);
    for (id, record) in state.nodes.iter() {
        let Some(node) = workflow.iter_nodes().find(|node| &node.id == id) else {
            continue;
        };
        if crate::worktree::lands_units(node) {
            landed = landed.max(record.last_terminal);
        }
        if builds_in_the_runs_tree(node) {
            built = built.max(record.last_started);
        }
    }
    landed.is_some_and(|landed| landed > bound && built.is_none_or(|built| landed > built))
}
