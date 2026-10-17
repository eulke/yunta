//! Why a node cannot proceed without the run tools: one answer for `check`,
//! which refuses a workflow no candidate adapter can serve, and for the run,
//! which refuses the session its adapter cannot.

/// Why a node cannot proceed without the run tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunToolsNeed {
    /// The node is in a `coordination: blackboard` group.
    Blackboard,
    /// The node declares an interpreted artifact a session hands over
    /// through the tools.
    TypedArtifact(yunta_core::ArtifactKind),
    /// The node is a loop: its task sessions read their task and check
    /// their work through the tools, and nowhere else.
    Task,
}

impl RunToolsNeed {
    /// The reason `node` owes, if any — one answer for `check`, which
    /// refuses a workflow no candidate adapter can serve, and for the
    /// run, which refuses the session its adapter cannot: a member of a
    /// blackboard group first, being the older reason, then a document it
    /// declares, then the loop it is.
    pub(crate) fn of(
        node: &yunta_core::Node,
        blackboard_member: bool,
        declared: &[yunta_core::ArtifactSpec],
    ) -> Option<Self> {
        if blackboard_member {
            return Some(RunToolsNeed::Blackboard);
        }
        if let Some(kind) = declared.iter().find_map(yunta_core::ArtifactSpec::kind) {
            return Some(RunToolsNeed::TypedArtifact(kind));
        }
        matches!(node.kind, yunta_core::NodeKind::Loop { .. }).then_some(RunToolsNeed::Task)
    }

    /// The declaration that asks for them, as the workflow spells it.
    pub(crate) fn declaration(&self) -> String {
        match self {
            RunToolsNeed::Blackboard => "coordination: blackboard".to_string(),
            RunToolsNeed::TypedArtifact(kind) => format!("artifacts.produces: [{kind}]"),
            RunToolsNeed::Task => "kind: loop".to_string(),
        }
    }
}
