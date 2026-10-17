//! What the engine does when an adapter does not declare a capability.
//!
//! A capability the engine consults and nobody decided about is a
//! capability that silently does nothing: three of them were consulted
//! by no one at all, and the five that were each answered in their own
//! words at their own call site. The answer belongs to the capability,
//! so it is written once, here, and the engine reads it rather than
//! deciding again.

use serde::{Deserialize, Serialize};

use crate::events::LoggedWords;
use crate::Capability;

/// What the engine does when an adapter does not declare a capability it
/// asked for. A closed set: the engine has one fallback per capability
/// it consults, and a degradation naming anything else would be a
/// fallback nobody implemented.
///
/// The words a reader is shown for each one are kept outside core, so
/// the same fallback reads the same way wherever it is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    /// `fence` is `None`: the session edits freely and the diff is
    /// judged afterwards.
    PostCheckOnly,
    /// `usage_reporting` absent: the run's token cap cannot be counted
    /// against this session.
    NoTokenBudget,
    /// `skills` absent: the session runs with none mounted.
    NoSkills,
    /// `run_tools` absent, or its server unreachable: the session holds
    /// none of the run's tools.
    NoRunTools,
    /// `network_isolation` absent: `network: false` is recorded, not
    /// enforced.
    NetworkOpen,
    /// `resume_session` absent, or nothing to resume: a fresh session
    /// replaces the interrupted one.
    FreshSession,
    /// `mid_session_token_cap` absent: the CLI reports no usage while a
    /// session runs, so the run's token cap is checked when the session ends.
    CapAtSessionEnd,
}

impl Policy {
    /// The word the log stores this policy under.
    pub fn as_str(self) -> &'static str {
        match self {
            Policy::PostCheckOnly => "post_check_only",
            Policy::NoTokenBudget => "no_token_budget",
            Policy::NoSkills => "no_skills",
            Policy::NoRunTools => "no_run_tools",
            Policy::NetworkOpen => "network_open",
            Policy::FreshSession => "fresh_session",
            Policy::CapAtSessionEnd => "cap_at_session_end",
        }
    }
}

/// What the log says the engine did: a closed policy, or a sentence an
/// older writer stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum PolicyApplied {
    Known(Policy),
    /// A sentence, kept as the writer stored it.
    Recorded(LoggedWords),
}

impl From<String> for PolicyApplied {
    /// The name of a closed policy reads back as that policy; any other
    /// text is a sentence, kept as written.
    fn from(text: String) -> Self {
        match serde_json::from_value::<Policy>(serde_json::Value::String(text.clone())) {
            Ok(policy) => PolicyApplied::Known(policy),
            Err(_) => PolicyApplied::Recorded(LoggedWords::read(text)),
        }
    }
}

impl From<PolicyApplied> for String {
    fn from(applied: PolicyApplied) -> Self {
        match applied {
            PolicyApplied::Known(policy) => policy.as_str().to_owned(),
            PolicyApplied::Recorded(text) => text.as_str().to_owned(),
        }
    }
}

/// What an absent capability means. Three answers, and every capability
/// has exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Absence {
    /// Nothing was asked for, so nothing is missing. The resting state
    /// of a capability a node never invoked.
    Resting,
    /// A workflow that asks for it cannot run at all: what it declared
    /// has no other way to happen, and emulating it would be a promise
    /// the engine does not keep. `yunta check` refuses it before a run
    /// is born, naming the node and the adapter, and a run that meets it
    /// anyway refuses the session.
    FailAtCheck,
    /// The run goes on under a stated fallback, recorded as
    /// `capability_degraded` so the log says what was asked for and what
    /// happened instead.
    DegradeWith(Policy),
}

/// Every capability and what its absence means. Exhaustive by test: a
/// capability added to [`Capability`] without a row here is a capability
/// whose absence nobody decided.
pub const POLICY: [(Capability, Absence); 10] = [
    (
        Capability::ResumeSession,
        Absence::DegradeWith(Policy::FreshSession),
    ),
    (
        Capability::Fence,
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
        // interpreted artifact, sits in a `coordination: blackboard`
        // group or is a loop has no other way in, and is refused rather
        // than run without what it declared. (`Policy::NoRunTools` is
        // what a listener that fails to bind records — a mount that
        // broke, not a capability the adapter never had.)
        Capability::RunTools,
        Absence::FailAtCheck,
    ),
    (
        Capability::NetworkIsolation,
        Absence::DegradeWith(Policy::NetworkOpen),
    ),
    (Capability::StopRefusals, Absence::FailAtCheck),
    (
        Capability::MidSessionTokenCap,
        Absence::DegradeWith(Policy::CapAtSessionEnd),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Each policy is stored under the name written here, and the name a
    /// serialized policy carries is the same one, so the log's word and the
    /// schema's word cannot drift apart.
    #[test]
    fn each_policy_is_stored_under_its_written_name() {
        let names = [
            (Policy::PostCheckOnly, "post_check_only"),
            (Policy::NoTokenBudget, "no_token_budget"),
            (Policy::NoSkills, "no_skills"),
            (Policy::NoRunTools, "no_run_tools"),
            (Policy::NetworkOpen, "network_open"),
            (Policy::FreshSession, "fresh_session"),
            (Policy::CapAtSessionEnd, "cap_at_session_end"),
        ];
        for (policy, name) in names {
            assert_eq!(policy.as_str(), name);
            assert_eq!(serde_json::to_value(policy).unwrap(), name);
            assert_eq!(String::from(PolicyApplied::Known(policy)), name);
        }
    }

    /// A sentence an older writer stored is kept as it was written, not
    /// matched against the closed set.
    #[test]
    fn a_stored_sentence_reads_back_as_written() {
        let sentence = "post-check only — the session edits unguarded".to_string();
        assert_eq!(
            PolicyApplied::from(sentence.clone()),
            PolicyApplied::Recorded(sentence.as_str().into())
        );
    }
}
