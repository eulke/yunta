//! What the engine does when an adapter does not declare a capability.
//!
//! A capability the engine consults and nobody decided about is a
//! capability that silently does nothing: three of them were consulted
//! by no one at all, and the five that were each answered in their own
//! words at their own call site. The answer belongs to the capability,
//! so it is written once, here, and the engine reads it rather than
//! deciding again.

use crate::events::Policy;
use crate::Capability;

/// What an absent capability means. Four answers, and every capability
/// has exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Absence {
    /// Nothing was asked for, so nothing is missing. The resting state
    /// of a capability a node never invoked.
    Resting,
    /// A workflow that asks for it cannot run at all: `yunta check`
    /// refuses it before a run is born, naming the node and the adapter.
    FailAtCheck,
    /// The node that asked for it fails: what it declared has no other
    /// way to happen, and emulating it would be a promise the engine
    /// does not keep.
    FailNode,
    /// The run goes on under a stated fallback, recorded as
    /// `capability_degraded` so the log says what was asked for and what
    /// happened instead.
    DegradeWith(Policy),
}

/// Every capability and what its absence means. Exhaustive by test: a
/// capability added to [`Capability`] without a row here is a capability
/// whose absence nobody decided.
pub const POLICY: [(Capability, Absence); 8] = [
    (
        Capability::ResumeSession,
        Absence::DegradeWith(Policy::FreshSession),
    ),
    (
        Capability::EditHooks,
        Absence::DegradeWith(Policy::PostCheckOnly),
    ),
    (Capability::PermissionProfiles, Absence::FailAtCheck),
    (Capability::CustomAgents, Absence::FailAtCheck),
    (
        Capability::UsageReporting,
        Absence::DegradeWith(Policy::NoTokenBudget),
    ),
    (Capability::Skills, Absence::DegradeWith(Policy::NoSkills)),
    (
        // The run's tools are an offer: a node that hands nothing over
        // and coordinates with nobody never consults this, so its
        // absence is nothing missing. A node that declares an
        // interpreted artifact or sits in a `coordination: blackboard`
        // group has no other way in, and is refused rather than run
        // without what it declared. (`Policy::NoRunTools` is what a
        // listener that fails to bind records — a mount that broke, not
        // a capability the adapter never had.)
        Capability::RunTools,
        Absence::FailNode,
    ),
    (
        Capability::NetworkIsolation,
        Absence::DegradeWith(Policy::NetworkOpen),
    ),
];

/// What this capability's absence means.
pub fn absence_of(capability: Capability) -> &'static Absence {
    // `POLICY` is exhaustive by test, so the fallback is unreachable —
    // and `Resting` is the reading that promises least, which is what an
    // unreachable branch should say.
    POLICY
        .iter()
        .find_map(|(named, absence)| (*named == capability).then_some(absence))
        .unwrap_or(&Absence::Resting)
}
