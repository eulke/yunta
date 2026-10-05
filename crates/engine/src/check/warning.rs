//! See [`super`]. What `check` reports without refusing the run.

use super::*;
use thiserror::Error;

/// What the run may still get through: a risk, a waste, a case its
/// author may be right about, or one only the run can settle — each
/// saying what would make it certain. The run starts. Kept separate from
/// `CheckError` rather than adding a severity field to it: every caller
/// of `check()` treats its `Vec<CheckError>` as "must be empty to
/// proceed" without learning to filter by severity.
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

    /// A read a mode answers only with what an earlier mode made: a run
    /// promoted into the mode is born holding it, and one started in it
    /// is not — which `yunta run --mode` refuses.
    #[error(
        "{site} reads {what}, which mode `{mode}` holds only when a run is promoted into it \
         from an earlier mode — a run started with `--mode {mode}` is refused for it"
    )]
    ReadOnlyThroughPromotion {
        site: super::Site,
        what: String,
        mode: yunta_core::ModeName,
    },

    /// `optional: true` on a node that needs nothing from the project:
    /// no config could leave it out, so the word promises nothing.
    #[error(
        "node `{node}` is declared `optional` and needs nothing the project declares — no \
         command, forge or other key — so no run leaves it out; drop `optional`"
    )]
    OptionalNeedsNothing { node: NodeId },

    /// A literal command starts a program this machine does not have on
    /// `PATH`. A warning: a node that runs earlier may install it, and a
    /// shell script read without a shell is read by heuristic.
    #[error(
        "node `{node}` runs `{program}`, which is not on this machine's `PATH` — install it \
         before the run reaches `{node}`, or it stops there"
    )]
    ProgramNotOnPath { node: NodeId, program: String },

    /// A literal `files:` path a node reads that the tree a run would
    /// start from does not hold — said before the first token, since the
    /// run only meets it once every node ahead of the reader has spent.
    #[error(
        "{}",
        super::context_files::missing_sentence(
            node,
            path,
            base.as_deref(),
            missing,
            super::context_files::Reach::EarlierNodeMay,
        )
    )]
    ContextFileMissing {
        node: NodeId,
        path: String,
        /// The commit an isolated run starts from, abbreviated; `None`
        /// when the run reads the checkout itself or the path is absolute.
        base: Option<String>,
        missing: super::context_files::MissingContextFile,
    },
}
