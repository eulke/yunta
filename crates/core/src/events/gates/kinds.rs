//! Which facts this domain records, and what each is called on the wire.

use super::payloads::*;

/// A decision a person makes: an escalation waiting on one, the — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum GateEvent {
    Waiting(GateWaitingPayload),
    Resolved(GateResolvedPayload),
    QuestionsAsked(QuestionsAskedPayload),
    QuestionsAnswered(QuestionsAnsweredPayload),
}

impl GateEvent {
    /// Every kind this domain declares, as persisted.
    pub const KINDS: &'static [&'static str] = &[
        "gate_waiting",
        "gate_resolved",
        "questions_asked",
        "questions_answered",
    ];

    /// The persisted `kind` string of this fact.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Waiting(_) => "gate_waiting",
            Self::Resolved(_) => "gate_resolved",
            Self::QuestionsAsked(_) => "questions_asked",
            Self::QuestionsAnswered(_) => "questions_answered",
        }
    }

    /// Whether this kind is audit: the log carries it so a reader can
    /// see what the engine did, and no ledger moves when it arrives.
    ///
    /// Every other kind moves state, and its ledger's `apply` names it.
    /// The pair is proved: `a_kind_that_moves_no_state_says_so_by_name`
    /// applies one of each to an empty state and checks that exactly the
    /// kinds answering `false` here change it.
    pub fn is_audit(&self) -> bool {
        match self {
            Self::Waiting(_) => false,
            Self::Resolved(_) => false,
            Self::QuestionsAsked(_) => false,
            Self::QuestionsAnswered(_) => false,
        }
    }

    /// The shape version of this kind. Every kind starts at 1 and a
    /// version is per kind, never per domain and never global: a kind
    /// that gains an incompatible shape becomes a new name, and only
    /// that one moves.
    pub fn schema_version(&self) -> u32 {
        1
    }
}
