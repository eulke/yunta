//! Where in the workflow file each refusal is about.
//!
//! A refusal says what is wrong in the workflow's own words; a surface
//! that quotes the file needs the place as well. The place is the node a
//! refusal names and, inside it, the key that says what it refuses — or
//! nothing, for a refusal about what the file does not hold: a config
//! key, a pack, another workflow.

use yunta_core::yaml::Pointer;
use yunta_core::{ConfigKey, NodeId};

use super::site::node;
use super::{CheckError, Unanswerable};

impl CheckError {
    /// Where in the workflow file this is about, from its root, or
    /// `None` when what it refuses is written somewhere else.
    pub fn pointer(&self) -> Option<Pointer> {
        let inputs =
            |name: &yunta_core::InputName| Pointer::root().key("inputs").key(name.as_str());
        let produces = |id: &NodeId| node(id).key("artifacts").key("produces");
        Some(match self {
            CheckError::DependsOnCycle { cycle } => node(cycle.first()?).key("depends_on"),
            CheckError::CapabilityUnsupported {
                node: id, field, ..
            } => node(id).key(field.as_str()),
            CheckError::UnknownRunner { node: id, .. }
            | CheckError::RunnerHasNoCandidates { node: id, .. } => node(id).key("runner"),
            CheckError::Unset { node: id, key } => match key {
                ConfigKey::Command { .. } => node(id).key("run"),
                ConfigKey::Executor { .. } => node(id).key("executor"),
                ConfigKey::Runner => node(id).key("runner"),
                ConfigKey::BaselineSuite
                | ConfigKey::Coverage
                | ConfigKey::Forge
                | ConfigKey::RunBranch => node(id),
            },
            CheckError::ContextFileUnreachable { node: id, .. }
            | CheckError::ContextOnUnsupportedNode { node: id } => node(id).key("context"),
            CheckError::Unanswerable(unanswerable) => unanswerable.pointer(),
            CheckError::BothRunnerAndRunners { node: id } => node(id).key("runners"),
            CheckError::FanOutTarget { node: id, .. }
            | CheckError::UndeclaredInput { node: id, .. }
            | CheckError::GateInsideParallel { node: id, .. }
            | CheckError::ParallelInsideParallel { node: id, .. }
            | CheckError::QuestionsInsideParallel { node: id, .. }
            | CheckError::GateRouteNeverTaken { node: id, .. }
            | CheckError::InheritChildWithoutScope { node: id, .. }
            | CheckError::ScopeExpansionModeOverCeiling { node: id, .. } => node(id),
            CheckError::DistillUnknownArtifact { .. } => Pointer::root().key("on_finish"),
            CheckError::DuplicateArtifactKind { node: id, .. }
            | CheckError::ArtifactNameRefused { node: id, .. }
            | CheckError::QuestionsAlongsideOtherArtifacts { node: id, .. }
            | CheckError::AnswersDeclaredAsProduced { node: id }
            | CheckError::QuestionsOnKind { node: id, .. } => produces(id),
            CheckError::InputDocumentAlsoProduced { input, .. } => inputs(input),
            CheckError::ReservedArtifactName { site, .. }
            | CheckError::AnswersFromNodeThatNeverAsks { site, .. } => site.pointer().clone(),
            CheckError::YuntaSchemaOutside { .. } => Pointer::root().key("yunta_schema"),
            CheckError::OverlappingFanOutScope { a, .. } => node(a).key("scope"),
            CheckError::CommandDenied { node: id, .. } => node(id).key("run"),
            CheckError::InputEmptyEnumValues { name } => inputs(name).key("values"),
            CheckError::InputMinExceedsMax { name, .. } => inputs(name).key("min"),
            CheckError::InputInvalidPattern { name, .. } => inputs(name).key("pattern"),
            CheckError::ExternalGateWithoutForge { node: id } => node(id).key("external"),
            CheckError::ShowsOnExternalGate { node: id } => node(id).key("shows"),
            CheckError::WorkflowNodeRunnerBinding { node: id, field } => node(id).key(*field),
            CheckError::MountOnSelf { node: id }
            | CheckError::MountOnFanOut { node: id, .. }
            | CheckError::MountInsideParallel { node: id, .. } => node(id).key("mounts"),
            CheckError::ResumeSessionOnSessionlessNode { node: id } => node(id).key("on_interrupt"),
            CheckError::WorkflowRefMissing { node: id, .. }
            | CheckError::AmbiguousWorkflowRef { node: id, .. }
            | CheckError::CrossPackWorkflowRef { node: id, .. }
            | CheckError::ComposedWorkflowFails { node: id, .. } => node(id).key("use"),
            CheckError::PackPermissionsCeilingExceeded { node: id, .. } => {
                node(id).key("permissions")
            }
            CheckError::PackManifestUnreadable { .. }
            | CheckError::PackManifestMalformed { .. }
            | CheckError::MaxParallelNodesZero
            | CheckError::WorkflowRefUnparseable { .. }
            | CheckError::BaselineWithoutSuite { .. }
            | CheckError::WorkflowRefCycle { .. }
            | CheckError::WorkflowRefTooDeep { .. }
            | CheckError::PackRequirementUnmet { .. } => return None,
        })
    }
}

impl Unanswerable {
    /// Where in the workflow file the read nothing answers is written.
    pub fn pointer(&self) -> Pointer {
        match self {
            Unanswerable::LoopWithoutTasks { node: id, .. } => node(id),
            Unanswerable::ArtifactNotDeclared { site, .. }
            | Unanswerable::ArtifactFromNowhere { site, .. }
            | Unanswerable::SourceLeftOut { site, .. } => site.pointer().clone(),
            Unanswerable::NodeOutputOfNonBash { node: id, .. } => node(id).key("context"),
        }
    }
}

/// `a -> b -> a`: a cycle as a sentence names it.
pub(super) fn arrows(cycle: &[NodeId]) -> String {
    cycle
        .iter()
        .map(NodeId::as_str)
        .collect::<Vec<_>>()
        .join(" -> ")
}
