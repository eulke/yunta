//! See [`super`]. What `check` reports without refusing the run.

use super::*;
use thiserror::Error;

/// A non-blocking finding — the run can still start (`check`
/// warns, it doesn't refuse, when a collision can't be verified for lack
/// of declared scope). Kept separate from `CheckError` rather than adding
/// a severity field to it: every existing caller of `check()` keeps
/// treating its `Vec<CheckError>` as "must be empty to proceed" without
/// learning to filter by severity.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CheckWarning {
    #[error(
        "parallel group `{group}`: two or more children can write and don't declare scope as \
         disjoint — the engine can't verify they won't collide; declare `scope` on each \
         to make the check real"
    )]
    UndeclaredParallelScope { group: NodeId },

    /// The fan-out analogue of `UndeclaredParallelScope` — one
    /// warning per connected component of mutually-independent,
    /// write-capable, scope-less top-level nodes (per pair would drown
    /// the signal in noise).
    #[error(
        "nodes {nodes} have no dependency paths between them and can all write without \
         declared scope — with `max_parallel_nodes` > 1 the engine can't verify they won't \
         collide; declare `scope` on each or chain them with `depends_on`"
    )]
    UndeclaredFanOutScope { nodes: String },

    /// A literal `git push` aimed at the base branch with no
    /// gate anywhere before it in the DAG — warning, not error: a team
    /// may genuinely want it, but nobody should discover an ungated
    /// push to `main` from the push itself.
    #[error(
        "node `{node}` pushes to the base branch (`{branch}`) with no gate anywhere before it \
         in the DAG — put a gate ahead of the push, or push to `{{{{run.branch}}}}`"
    )]
    PushToBaseWithoutGate { node: NodeId, branch: String },
}
