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

    /// A literal `files:` path a node reads that the tree a run would
    /// start from does not hold — said before the first token, since the
    /// run only meets it once every node ahead of the reader has spent.
    #[error("{}", context_file_missing(node, path, base.as_deref(), missing))]
    ContextFileMissing {
        node: NodeId,
        path: String,
        /// The commit an isolated run starts from, abbreviated; `None`
        /// when the run reads the checkout itself or the path is absolute.
        base: Option<String>,
        missing: super::context_files::MissingContextFile,
    },
}

/// The sentence for a missing `files:` path: what is missing, why the run
/// would not see it, and what to do — each shape its own remedy.
fn context_file_missing(
    node: &NodeId,
    path: &str,
    base: Option<&str>,
    missing: &super::context_files::MissingContextFile,
) -> String {
    use super::context_files::MissingContextFile as M;
    let reads = format!("node `{node}` reads `{path}` (a `files:` context source)");
    let stops = format!("unless a node before it writes the file, `{node}` stops there");
    match (missing, base) {
        (M::Nowhere, Some(base)) => format!(
            "{reads}, which commit `{base}` — the one a run starts from — does not hold: \
             {stops}; commit the file first"
        ),
        (M::Nowhere, None) => format!("{reads}, which does not exist: {stops}"),
        (M::Uncommitted, _) => format!(
            "{reads}, which is in your checkout but not committed: a run starts from commit \
             `{}` and never sees it, so {stops}; commit it first",
            base.unwrap_or("HEAD")
        ),
        (M::Ignored, _) => format!(
            "{reads}, which git ignores: a run's tree never carries an ignored file, so \
             {stops}; add it with `git add -f`, or read a file git tracks"
        ),
    }
}
