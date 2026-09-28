//! Why a node's sessions cannot open without the run tools — one answer
//! for `check`, which refuses a workflow no candidate adapter can serve,
//! and for the run, which refuses the session when its adapter cannot.

use yunta_core::{ArtifactKind, ArtifactSpec, Node, NodeKind};

/// What a node reaches only through the run tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunToolsNeed {
    /// A `coordination: blackboard` group's members post and read
    /// through them, and the engine never emulates the blackboard.
    Blackboard,
    /// A document the engine reads reaches it from a session through
    /// them and nowhere else.
    Document(ArtifactKind),
    /// A loop's task sessions read their task and check their work
    /// through them.
    Tasks,
}

impl RunToolsNeed {
    /// The reason `node` owes, if any: a member of a blackboard group
    /// first, being the older reason, then a document it declares, then
    /// the loop it is.
    pub(crate) fn of(
        node: &Node,
        blackboard_member: bool,
        declared: &[ArtifactSpec],
    ) -> Option<Self> {
        if blackboard_member {
            return Some(RunToolsNeed::Blackboard);
        }
        if let Some(kind) = declared.iter().find_map(ArtifactSpec::kind) {
            return Some(RunToolsNeed::Document(kind));
        }
        matches!(node.kind, NodeKind::Loop { .. }).then_some(RunToolsNeed::Tasks)
    }

    /// The declaration that asks for them, as the workflow spells it.
    pub(crate) fn declaration(self) -> String {
        match self {
            RunToolsNeed::Blackboard => "coordination: blackboard".to_string(),
            RunToolsNeed::Document(kind) => format!("artifacts.produces: [{kind}]"),
            RunToolsNeed::Tasks => "kind: loop".to_string(),
        }
    }
}
