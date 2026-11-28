//! See [`super`]. The runner roles a repository's setup proposes, read from its
//! workflows: each role they name, once, in the order their nodes name it, and the
//! default runner where a node names none; or, where they name no runner, the one role
//! that stands for whatever such a node runs on, which is the default runner too.

use yunta_core::{ConfigKey, NonEmpty, RunnerName, Workflow};

/// The one role a repository whose workflows name none is proposed, as
/// [`ProposedRunners::roles`] lends it.
static STAND_IN: RunnerName = RunnerName::PROPOSED;

/// The runner roles a repository's setup proposes, and which of them a node that
/// names no runner runs on, the default runner (`defaults.runner`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposedRunners {
    /// The roles the repository's workflows name, each once, in the order their
    /// nodes name them; `needs_default` where a node names none, so the first is the
    /// default runner.
    Named {
        roles: NonEmpty<RunnerName>,
        needs_default: bool,
    },
    /// No workflow names a runner: the one role, [`RunnerName::PROPOSED`], that stands
    /// for whatever a node naming none runs on, and so is the default runner too.
    StandIn,
}

impl ProposedRunners {
    /// What `workflows` propose, each role once, in the order their nodes name
    /// them; the stand-in where they name none.
    pub(super) fn of(workflows: &[Workflow]) -> Self {
        let mut roles: Vec<RunnerName> = Vec::new();
        let mut needs_default = false;
        for node in workflows.iter().flat_map(Workflow::iter_nodes) {
            for role in node.runner.iter().chain(&node.runners) {
                if !roles.contains(role) {
                    roles.push(role.clone());
                }
            }
            needs_default |= ConfigKey::needed_by(node).contains(&ConfigKey::Runner);
        }
        match NonEmpty::new(roles) {
            Some(roles) => ProposedRunners::Named {
                roles,
                needs_default,
            },
            None => ProposedRunners::StandIn,
        }
    }

    /// Each role proposed, in the order it is proposed.
    pub fn roles(&self) -> &[RunnerName] {
        match self {
            ProposedRunners::Named { roles, .. } => roles.as_slice(),
            ProposedRunners::StandIn => std::slice::from_ref(&STAND_IN),
        }
    }

    /// The role a node that names no runner runs on, which `defaults.runner` names:
    /// the stand-in, or the first role named where a node names none; none otherwise.
    pub fn default_runner(&self) -> Option<&RunnerName> {
        match self {
            ProposedRunners::Named {
                roles,
                needs_default: true,
            } => roles.as_slice().first(),
            ProposedRunners::Named { .. } => None,
            ProposedRunners::StandIn => Some(&STAND_IN),
        }
    }
}
