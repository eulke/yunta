//! What a failure hands its readers: the lines its command printed, the run
//! object holding its output, the paths it wrote outside its scope, and the
//! artifacts that did not close. Each accessor names only the variants that
//! carry what it reads; every other failure carries none of it.

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
            Failure::Unchanged { unchanged } => unchanged.failure.tail(),
            Failure::ChildFailed {
                child_failure: Some(child),
                ..
            } => child.tail(),
            Failure::Red {
                red: RedCheck::Regression { tail, .. },
            } => tail,
            _ => &[],
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
            _ => None,
        }
    }

    /// The paths the node wrote outside its scope. Empty for a failure
    /// that is not a violation of it. A child run's violations are its
    /// own, and do not widen this node's scope.
    pub fn outside_scope(&self) -> &[PathBuf] {
        match self {
            Failure::ScopeViolated { outside_scope } => outside_scope,
            Failure::Unchanged { unchanged } => unchanged.failure.outside_scope(),
            _ => &[],
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
            _ => false,
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
            _ => self
                .outside_scope()
                .iter()
                .map(|path| ScopeGlob::exact(path))
                .collect(),
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
            _ => [].iter(),
        }
    }
}
