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
         disjoint — they share the run's tree, so the last write wins and nothing says so; \
         declare `scope` on each, which gives every child a checkout of its own and its \
         writes a boundary this engine enforces"
    )]
    UndeclaredParallelScope { group: NodeId },

    /// The fan-out analogue of `UndeclaredParallelScope` — one
    /// warning per connected component of mutually-independent,
    /// write-capable, scope-less top-level nodes (per pair would drown
    /// the signal in noise).
    #[error(
        "nodes {nodes} have no dependency paths between them and can all write without \
         declared scope — with `max_parallel_nodes` > 1 they share the run's tree at the \
         same moment and the last write wins; declare `scope` on each, which gives every \
         one a checkout of its own, or chain them with `depends_on`"
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

    /// A suite nobody reads. The measurement runs before the first node
    /// of the run whatever the workflow does with it, so a config that
    /// names one and a composition that never compares is minutes spent
    /// on an answer no check asks for.
    #[error(
        "config declares `baseline.suite` (`{suite}`) and no node of this workflow or of the \
         workflows it composes is a `baseline_compare`: run on its own, this workflow measures \
         the suite before its first node and nothing reads the measurement — add the check, or \
         drop the suite"
    )]
    BaselineNeverCompared { suite: String },
}
