//! What `check` and a run say without refusing them, as data. A caution
//! names what a person may want to change; the words are written where
//! they are read.

use serde::{Deserialize, Serialize};

use crate::events::ArtifactId;
use crate::ids::{ModeName, NodeId};
use crate::workflow::Place;

/// Something about a workflow or a run that a person may want to change,
/// which does not stop the run: each one says what it is about.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Caution {
    /// The declared cap sits under what past runs of this workflow spent at p90.
    BudgetBelowHistory { cap: u64, p90: f64, samples: usize },
    /// Parallel children that can write and declare no scope, so they share one tree.
    UndeclaredParallelScope { group: NodeId },
    /// Independent writers with no declared scope, which can run at the same time.
    UndeclaredFanOutScope { nodes: Vec<NodeId> },
    /// A push to the base branch with no gate anywhere ahead of it.
    PushToBaseWithoutGate { node: NodeId, branch: String },
    /// A baseline suite that no node compares against.
    BaselineNeverCompared { suite: String },
    /// A read a mode answers only with what a promotion into it brings.
    ReadOnlyThroughPromotion {
        place: Place,
        what: PromotedRead,
        mode: ModeName,
    },
    /// An optional node that needs nothing the project declares, so no run leaves it out.
    OptionalNeedsNothing { node: NodeId },
    /// A program a node runs that is not on this machine's path.
    ProgramNotOnPath { node: NodeId, program: String },
    /// A file a node reads that the tree the run starts from does not hold.
    ContextFileMissing {
        node: NodeId,
        path: String,
        /// The commit an isolated run starts from, abbreviated; `None`
        /// when the run reads the checkout itself or the path is absolute.
        base: Option<String>,
        missing: MissingContextFile,
    },
    /// The adapter cannot check the token cap per line: it is checked when the session completes.
    TokenCapAtSessionEnd { node: NodeId },
}

/// What a mode reads that only a promotion into it holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PromotedRead {
    /// An artifact of the run, whichever node produced it.
    Artifact(ArtifactId),
    /// A node the reader takes its input from.
    Source(NodeId),
}

/// How a `files:` path that the run would not find is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MissingContextFile {
    /// Neither in the commit the run starts from nor on disk.
    Nowhere,
    /// On disk in the person's checkout, and not committed.
    Uncommitted,
    /// On disk in the person's checkout, and ignored by git.
    Ignored,
}
