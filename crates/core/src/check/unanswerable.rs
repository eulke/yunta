//! See [`super`]. A read the run cannot answer.

use crate::events::ArtifactId;
use crate::workflow::Place;
use crate::{ModeName, NodeId};

/// A read nothing in the run can answer: what it asks for, and who asks.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unanswerable {
    /// A loop works through the run's tasks document, and nothing in
    /// the graph a run of this workflow can be gives it one.
    #[error("unanswerable_loop_without_tasks")]
    LoopWithoutTasks {
        node: NodeId,
        mode: Option<ModeName>,
    },

    /// A reference that names the node it reads from reaches that
    /// node's artifacts and nothing else, and the node does not declare
    /// this one.
    #[error("unanswerable_artifact_not_declared")]
    ArtifactNotDeclared {
        place: Place,
        node: NodeId,
        artifact: ArtifactId,
    },

    /// A reference that asks the run for an artifact, whichever node
    /// produced it, and nothing in the run can hold one.
    #[error("unanswerable_artifact_from_nowhere")]
    ArtifactFromNowhere {
        place: Place,
        artifact: ArtifactId,
        mode: Option<ModeName>,
    },

    /// A run started in a mode reads from a node the mode leaves out:
    /// only a run promoted into the mode would hold what that node made.
    #[error("unanswerable_source_left_out")]
    SourceLeftOut {
        place: Place,
        from: NodeId,
        mode: ModeName,
    },

    /// Only a `kind: bash` node captures the output a `node-output:`
    /// source reads.
    #[error("unanswerable_node_output_of_non_bash")]
    NodeOutputOfNonBash {
        node: NodeId,
        referenced: NodeId,
        kind: Option<&'static str>,
    },
}
