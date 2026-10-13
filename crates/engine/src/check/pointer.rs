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
use super::{CheckError, Composition, Unanswerable};

impl CheckError {
    /// Where in the workflow file this is about, from its root, or
    /// `None` when what it refuses is written somewhere else.
    pub fn pointer(&self) -> Option<Pointer> {
        self.node_pointer().or_else(|| self.document_pointer())
    }

    /// The place of a refusal about one node: the node, or the key of it
    /// that the refusal is about. `None` for a refusal that names no node.
    fn node_pointer(&self) -> Option<Pointer> {
        Some(match self {
            CheckError::DependsOnCycle { cycle } => node(cycle.first()?).key("depends_on"),
            CheckError::CapabilityUnsupported {
                node: id, field, ..
            } => node(id).key(field.as_str()),
            CheckError::UnknownRunner { node: id, .. }
            | CheckError::RunnerHasNoCandidates { node: id, .. } => node(id).key("runner"),
            CheckError::Unset { node: id, key, .. } => unset(id, key),
            CheckError::ContextFileUnreachable { node: id, .. }
            | CheckError::ContextOnUnsupportedNode { node: id } => node(id).key("context"),
            CheckError::BothRunnerAndRunners { node: id } => node(id).key("runners"),
            CheckError::FanOutTarget { node: id, .. }
            | CheckError::UndeclaredInput { node: id, .. }
            | CheckError::GateInsideParallel { node: id, .. }
            | CheckError::ParallelInsideParallel { node: id, .. }
            | CheckError::QuestionsInsideParallel { node: id, .. }
            | CheckError::GateRouteNeverTaken { node: id, .. }
            | CheckError::InheritChildWithoutScope { node: id, .. }
            | CheckError::ScopeExpansionModeOverCeiling { node: id, .. } => node(id),
            CheckError::OverlappingFanOutScope { a, .. } => node(a).key("scope"),
            CheckError::CommandDenied { node: id, .. } => node(id).key("run"),
            CheckError::ExternalGateWithoutForge { node: id } => node(id).key("external"),
            CheckError::ShowsOnExternalGate { node: id } => node(id).key("shows"),
            CheckError::WorkflowNodeRunnerBinding { node: id, field } => node(id).key(*field),
            CheckError::MountOnSelf { node: id }
            | CheckError::MountOnFanOut { node: id, .. }
            | CheckError::MountInsideParallel { node: id, .. } => node(id).key("mounts"),
            CheckError::ResumeSessionOnSessionlessNode { node: id } => node(id).key("on_interrupt"),
            CheckError::AskOnANodeThatAsksNothing { node: id } => node(id).key("ask"),
            CheckError::NetworkOpenUnderReadOnly { node: id }
            | CheckError::ClosedNetworkUnconfined { node: id }
            | CheckError::PackNetworkCeilingExceeded { node: id, .. } => node(id).key("network"),
            CheckError::ScopeExpansionWithinRetired { node: id } => {
                node(id).key("scope_expansion").key("within")
            }
            CheckError::Composition(composition) => composition.pointer()?,
            CheckError::PackPermissionsCeilingExceeded { node: id, .. } => {
                node(id).key("permissions")
            }
            _ => return None,
        })
    }

    /// The place of a refusal about what a node produces or reads, a key of
    /// the document, or an input. `None` for a refusal that names none of
    /// these.
    fn document_pointer(&self) -> Option<Pointer> {
        let inputs =
            |name: &yunta_core::InputName| Pointer::root().key("inputs").key(name.as_str());
        let produces = |id: &NodeId| node(id).key("artifacts").key("produces");
        Some(match self {
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
            CheckError::InputEmptyEnumValues { name } => inputs(name).key("values"),
            CheckError::InputMinExceedsMax { name, .. } => inputs(name).key("min"),
            CheckError::InputInvalidPattern { name, .. } => inputs(name).key("pattern"),
            CheckError::Unanswerable(unanswerable) => unanswerable.pointer(),
            _ => return None,
        })
    }
}

/// Where a config key left unset is named in its node: the node's own key
/// that sets it, or the node itself for a key the project configures.
fn unset(id: &NodeId, key: &ConfigKey) -> Pointer {
    match key {
        ConfigKey::Command { .. } => node(id).key("run").key("command"),
        ConfigKey::Executor { .. } => node(id).key("executor"),
        ConfigKey::Runner => node(id).key("runner"),
        ConfigKey::BaselineSuite
        | ConfigKey::Coverage
        | ConfigKey::Forge
        | ConfigKey::RunBranch => node(id),
    }
}

impl Composition {
    /// The `use:` a composition refusal names in the node that composes;
    /// the refusals about a cycle, a depth or a file that does not parse
    /// name none.
    fn pointer(&self) -> Option<Pointer> {
        match self {
            Composition::Missing { node: id, .. }
            | Composition::Ambiguous { node: id, .. }
            | Composition::CrossPack { node: id, .. }
            | Composition::Fails { node: id, .. } => Some(node(id).key("use")),
            Composition::Unparseable { .. }
            | Composition::Cycle { .. }
            | Composition::TooDeep { .. } => None,
        }
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

/// ` — did you mean `name`?` when a refusal holds a near miss.
pub(super) fn near(near: Option<&String>) -> String {
    near.map(|near| format!(" — did you mean `{near}`?"))
        .unwrap_or_default()
}

/// `a -> b -> a`: a cycle as a sentence names it.
pub(super) fn arrows(cycle: &[NodeId]) -> String {
    cycle
        .iter()
        .map(NodeId::as_str)
        .collect::<Vec<_>>()
        .join(" -> ")
}
