//! What a failure hands its readers: the lines its command printed, the run
//! object holding its output, the paths it wrote outside its scope, and the
//! artifacts that did not close. Each accessor names every variant: the ones
//! that carry what it reads answer with it, and the rest are listed with the
//! empty answer, so a variant added later is answered on purpose.

use std::path::PathBuf;

use crate::diagnostic::{ArtifactFailure, Report};
use crate::glob::{InvalidScopeGlob, ScopeGlob};
use crate::hash::ContentHash;

use super::{Failure, RedCheck};

impl Failure {
    /// The last lines the failing command printed, as the log holds them
    /// here. Empty for a failure that is not about a command, for a command
    /// whose output the log holds as a run object, and for one that printed
    /// nothing. A child run's failure hands over its own command's lines,
    /// which a person reads under this one.
    pub fn tail(&self) -> &[String] {
        match self {
            Failure::Exited { exited } => &exited.tail,
            Failure::Red {
                red: RedCheck::Regression { tail, .. },
            } => tail,
            Failure::Unchanged { unchanged } => unchanged.failure.tail(),
            Failure::ChildFailed {
                child_failure: Some(child),
                ..
            } => child.tail(),
            Failure::Red {
                red: RedCheck::Command { .. } | RedCheck::Coverage { .. },
            } => &[],
            Failure::ChildFailed {
                child_failure: None,
                ..
            } => &[],
            Failure::Artifacts { .. } | Failure::SessionDied { .. } => &[],
            Failure::ScopeViolated { .. } | Failure::ScopeRequested { .. } => &[],
            Failure::ScopeOwed { .. } | Failure::Unset { .. } => &[],
            Failure::PathsDenied { .. } | Failure::PlanUnpassable { .. } => &[],
            Failure::SessionRefused { .. } | Failure::Limit { .. } => &[],
            Failure::TasksStopped { .. } | Failure::FrozenInput { .. } => &[],
            Failure::SessionNotStopped { .. } | Failure::TimedOut { .. } => &[],
            Failure::TokenBudget { .. } | Failure::Template { .. } => &[],
            Failure::ArtifactUnreadable { .. } | Failure::ArtifactName { .. } => &[],
            Failure::MountUnreachable { .. } | Failure::GateChose { .. } => &[],
            Failure::TestsAccepted { .. } | Failure::ChildRefused { .. } => &[],
            Failure::NestedTooDeep { .. } | Failure::GateArtifactMissing { .. } => &[],
            Failure::CapabilityMissing { .. } | Failure::EngineFailed { .. } => &[],
            Failure::CommandDenied { .. } | Failure::SkillMissing { .. } => &[],
            Failure::LoopCapped { .. } | Failure::Confinement { .. } => &[],
            Failure::FrozenRead { .. } | Failure::ChildrenFailed { .. } => &[],
            Failure::Uncertain { .. } | Failure::Rejected { .. } => &[],
            Failure::SentBack { .. } | Failure::FindingsOpen { .. } => &[],
            Failure::Interrupted { .. } | Failure::ChangesRequested { .. } => &[],
            Failure::PullRequestClosed { .. } | Failure::LoopStalled { .. } => &[],
            Failure::NoTasksDocument { .. } | Failure::RunnerUnresolved { .. } => &[],
            Failure::Message { .. } => &[],
        }
    }

    /// The run object holding everything the failing command printed.
    pub fn output(&self) -> Option<&ContentHash> {
        match self {
            Failure::Exited { exited } => exited.output.as_ref(),
            Failure::Red { red } => match red {
                RedCheck::Command { output, .. } => output.as_ref(),
                RedCheck::Coverage { .. } | RedCheck::Regression { .. } => None,
            },
            Failure::Unchanged { unchanged } => unchanged.failure.output(),
            Failure::ChildFailed {
                child_failure: Some(child),
                ..
            } => child.output(),
            Failure::ChildFailed {
                child_failure: None,
                ..
            } => None,
            Failure::Artifacts { .. } | Failure::SessionDied { .. } => None,
            Failure::ScopeViolated { .. } | Failure::ScopeRequested { .. } => None,
            Failure::ScopeOwed { .. } | Failure::Unset { .. } => None,
            Failure::PathsDenied { .. } | Failure::PlanUnpassable { .. } => None,
            Failure::SessionRefused { .. } | Failure::Limit { .. } => None,
            Failure::TasksStopped { .. } | Failure::FrozenInput { .. } => None,
            Failure::SessionNotStopped { .. } | Failure::TimedOut { .. } => None,
            Failure::TokenBudget { .. } | Failure::Template { .. } => None,
            Failure::ArtifactUnreadable { .. } | Failure::ArtifactName { .. } => None,
            Failure::MountUnreachable { .. } | Failure::GateChose { .. } => None,
            Failure::TestsAccepted { .. } | Failure::ChildRefused { .. } => None,
            Failure::NestedTooDeep { .. } | Failure::GateArtifactMissing { .. } => None,
            Failure::CapabilityMissing { .. } | Failure::EngineFailed { .. } => None,
            Failure::CommandDenied { .. } | Failure::SkillMissing { .. } => None,
            Failure::LoopCapped { .. } | Failure::Confinement { .. } => None,
            Failure::FrozenRead { .. } | Failure::ChildrenFailed { .. } => None,
            Failure::Uncertain { .. } | Failure::Rejected { .. } => None,
            Failure::SentBack { .. } | Failure::FindingsOpen { .. } => None,
            Failure::Interrupted { .. } | Failure::ChangesRequested { .. } => None,
            Failure::PullRequestClosed { .. } | Failure::LoopStalled { .. } => None,
            Failure::NoTasksDocument { .. } | Failure::RunnerUnresolved { .. } => None,
            Failure::Message { .. } => None,
        }
    }

    /// The paths the node wrote outside its scope. Empty for a failure
    /// that is not a violation of it. A child run's violations are its
    /// own, and do not widen this node's scope.
    pub fn outside_scope(&self) -> &[PathBuf] {
        match self {
            Failure::ScopeViolated { outside_scope } => outside_scope,
            Failure::Unchanged { unchanged } => unchanged.failure.outside_scope(),
            Failure::Artifacts { .. } | Failure::SessionDied { .. } => &[],
            Failure::ScopeRequested { .. } | Failure::ScopeOwed { .. } => &[],
            Failure::Unset { .. } | Failure::PathsDenied { .. } => &[],
            Failure::Exited { .. } | Failure::PlanUnpassable { .. } => &[],
            Failure::SessionRefused { .. } | Failure::Limit { .. } => &[],
            Failure::TasksStopped { .. } | Failure::Red { .. } => &[],
            Failure::ChildFailed { .. } | Failure::FrozenInput { .. } => &[],
            Failure::SessionNotStopped { .. } | Failure::TimedOut { .. } => &[],
            Failure::TokenBudget { .. } | Failure::Template { .. } => &[],
            Failure::ArtifactUnreadable { .. } | Failure::ArtifactName { .. } => &[],
            Failure::MountUnreachable { .. } | Failure::GateChose { .. } => &[],
            Failure::TestsAccepted { .. } | Failure::ChildRefused { .. } => &[],
            Failure::NestedTooDeep { .. } | Failure::GateArtifactMissing { .. } => &[],
            Failure::CapabilityMissing { .. } | Failure::EngineFailed { .. } => &[],
            Failure::CommandDenied { .. } | Failure::SkillMissing { .. } => &[],
            Failure::LoopCapped { .. } | Failure::Confinement { .. } => &[],
            Failure::FrozenRead { .. } | Failure::ChildrenFailed { .. } => &[],
            Failure::Uncertain { .. } | Failure::Rejected { .. } => &[],
            Failure::SentBack { .. } | Failure::FindingsOpen { .. } => &[],
            Failure::Interrupted { .. } | Failure::ChangesRequested { .. } => &[],
            Failure::PullRequestClosed { .. } | Failure::LoopStalled { .. } => &[],
            Failure::NoTasksDocument { .. } | Failure::RunnerUnresolved { .. } => &[],
            Failure::Message { .. } => &[],
        }
    }

    /// Whether a wider scope is what this failure needs — what makes a
    /// grant a way forward rather than a guess.
    pub fn wants_scope(&self) -> bool {
        match self {
            Failure::ScopeViolated { outside_scope } => !outside_scope.is_empty(),
            Failure::ScopeRequested { requested_scope } => !requested_scope.paths.is_empty(),
            Failure::ScopeOwed { owed } => owed.iter().any(|owed| !owed.paths.is_empty()),
            Failure::Unchanged { unchanged } => unchanged.failure.wants_scope(),
            Failure::Artifacts { .. } | Failure::SessionDied { .. } => false,
            Failure::Unset { .. } | Failure::Exited { .. } => false,
            Failure::PathsDenied { .. } | Failure::PlanUnpassable { .. } => false,
            Failure::SessionRefused { .. } | Failure::Limit { .. } => false,
            Failure::TasksStopped { .. } | Failure::Red { .. } => false,
            Failure::ChildFailed { .. } | Failure::FrozenInput { .. } => false,
            Failure::SessionNotStopped { .. } | Failure::TimedOut { .. } => false,
            Failure::TokenBudget { .. } | Failure::Template { .. } => false,
            Failure::ArtifactUnreadable { .. } | Failure::ArtifactName { .. } => false,
            Failure::MountUnreachable { .. } | Failure::GateChose { .. } => false,
            Failure::TestsAccepted { .. } | Failure::ChildRefused { .. } => false,
            Failure::NestedTooDeep { .. } | Failure::GateArtifactMissing { .. } => false,
            Failure::CapabilityMissing { .. } | Failure::EngineFailed { .. } => false,
            Failure::CommandDenied { .. } | Failure::SkillMissing { .. } => false,
            Failure::LoopCapped { .. } | Failure::Confinement { .. } => false,
            Failure::FrozenRead { .. } | Failure::ChildrenFailed { .. } => false,
            Failure::Uncertain { .. } | Failure::Rejected { .. } => false,
            Failure::SentBack { .. } | Failure::FindingsOpen { .. } => false,
            Failure::Interrupted { .. } | Failure::ChangesRequested { .. } => false,
            Failure::PullRequestClosed { .. } | Failure::LoopStalled { .. } => false,
            Failure::NoTasksDocument { .. } | Failure::RunnerUnresolved { .. } => false,
            Failure::Message { .. } => false,
        }
    }

    /// What a grant would add to the node's scope for the work behind
    /// this failure to stand: each path it wrote outside, exactly, or
    /// what its session asked for. Empty for a failure a wider scope
    /// would not change.
    pub fn scope_wanted(&self) -> Result<Vec<ScopeGlob>, InvalidScopeGlob> {
        match self {
            Failure::ScopeRequested { requested_scope } => Ok(requested_scope.paths.clone()),
            // Each task's request is its own to grant: none of it widens
            // the loop's scope.
            Failure::ScopeOwed { .. } => Ok(Vec::new()),
            Failure::ScopeViolated { .. } | Failure::Unchanged { .. } => self
                .outside_scope()
                .iter()
                .map(|path| ScopeGlob::exact(path))
                .collect(),
            Failure::Artifacts { .. } | Failure::SessionDied { .. } => Ok(Vec::new()),
            Failure::Unset { .. } | Failure::Exited { .. } => Ok(Vec::new()),
            Failure::PathsDenied { .. } | Failure::PlanUnpassable { .. } => Ok(Vec::new()),
            Failure::SessionRefused { .. } | Failure::Limit { .. } => Ok(Vec::new()),
            Failure::TasksStopped { .. } | Failure::Red { .. } => Ok(Vec::new()),
            Failure::ChildFailed { .. } | Failure::FrozenInput { .. } => Ok(Vec::new()),
            Failure::SessionNotStopped { .. } | Failure::TimedOut { .. } => Ok(Vec::new()),
            Failure::TokenBudget { .. } | Failure::Template { .. } => Ok(Vec::new()),
            Failure::ArtifactUnreadable { .. } | Failure::ArtifactName { .. } => Ok(Vec::new()),
            Failure::MountUnreachable { .. } | Failure::GateChose { .. } => Ok(Vec::new()),
            Failure::TestsAccepted { .. } | Failure::ChildRefused { .. } => Ok(Vec::new()),
            Failure::NestedTooDeep { .. } | Failure::GateArtifactMissing { .. } => Ok(Vec::new()),
            Failure::CapabilityMissing { .. } | Failure::EngineFailed { .. } => Ok(Vec::new()),
            Failure::CommandDenied { .. } | Failure::SkillMissing { .. } => Ok(Vec::new()),
            Failure::LoopCapped { .. } | Failure::Confinement { .. } => Ok(Vec::new()),
            Failure::FrozenRead { .. } | Failure::ChildrenFailed { .. } => Ok(Vec::new()),
            Failure::Uncertain { .. } | Failure::Rejected { .. } => Ok(Vec::new()),
            Failure::SentBack { .. } | Failure::FindingsOpen { .. } => Ok(Vec::new()),
            Failure::Interrupted { .. } | Failure::ChangesRequested { .. } => Ok(Vec::new()),
            Failure::PullRequestClosed { .. } | Failure::LoopStalled { .. } => Ok(Vec::new()),
            Failure::NoTasksDocument { .. } | Failure::RunnerUnresolved { .. } => Ok(Vec::new()),
            Failure::Message { .. } => Ok(Vec::new()),
        }
    }

    /// Every report behind this failure, each carrying the document it
    /// is about. What a diagnostic is rendered from; a failure whose
    /// artifacts name no document yields none.
    pub fn reports(&self) -> impl Iterator<Item = &Report> {
        self.failures().filter_map(ArtifactFailure::report)
    }

    /// Every declared artifact that did not close. Empty for a failure
    /// that has nothing to do with artifacts.
    pub fn failures(&self) -> impl Iterator<Item = &ArtifactFailure> {
        match self {
            Failure::Artifacts { artifacts } => artifacts.iter(),
            Failure::Unchanged { unchanged } => unchanged.failure.failures(),
            // A dead session names no artifact, and neither does a scope, a
            // command or a sentence. A refused session, a limit, a loop's
            // stops, a check's figure and a child run's failure name no
            // document this node declared either: the count of documents
            // that did not close is about documents this node declared.
            Failure::SessionDied { .. } | Failure::ScopeViolated { .. } => [].iter(),
            Failure::ScopeRequested { .. } | Failure::ScopeOwed { .. } => [].iter(),
            Failure::Unset { .. } | Failure::PathsDenied { .. } => [].iter(),
            Failure::Exited { .. } | Failure::PlanUnpassable { .. } => [].iter(),
            Failure::SessionRefused { .. } | Failure::Limit { .. } => [].iter(),
            Failure::TasksStopped { .. } | Failure::Red { .. } => [].iter(),
            Failure::ChildFailed { .. } | Failure::FrozenInput { .. } => [].iter(),
            Failure::SessionNotStopped { .. } | Failure::TimedOut { .. } => [].iter(),
            Failure::TokenBudget { .. } | Failure::Template { .. } => [].iter(),
            Failure::ArtifactUnreadable { .. } | Failure::ArtifactName { .. } => [].iter(),
            Failure::MountUnreachable { .. } | Failure::GateChose { .. } => [].iter(),
            Failure::TestsAccepted { .. } | Failure::ChildRefused { .. } => [].iter(),
            Failure::NestedTooDeep { .. } | Failure::GateArtifactMissing { .. } => [].iter(),
            Failure::CapabilityMissing { .. } | Failure::EngineFailed { .. } => [].iter(),
            Failure::CommandDenied { .. } | Failure::SkillMissing { .. } => [].iter(),
            Failure::LoopCapped { .. } | Failure::Confinement { .. } => [].iter(),
            Failure::FrozenRead { .. } | Failure::ChildrenFailed { .. } => [].iter(),
            Failure::Uncertain { .. } | Failure::Rejected { .. } => [].iter(),
            Failure::SentBack { .. } | Failure::FindingsOpen { .. } => [].iter(),
            Failure::Interrupted { .. } | Failure::ChangesRequested { .. } => [].iter(),
            Failure::PullRequestClosed { .. } | Failure::LoopStalled { .. } => [].iter(),
            Failure::NoTasksDocument { .. } | Failure::RunnerUnresolved { .. } => [].iter(),
            Failure::Message { .. } => [].iter(),
        }
    }
}
