//! Which facts this domain records, and what each is called on the wire.

use super::payloads::*;

/// What a session reported about work that is not its own to fix: posted, — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum FindingEvent {
    Posted(FindingPostedPayload),
    Updated(FindingUpdatedPayload),
    Withdrawn(FindingWithdrawnPayload),
    Refused(FindingRefusedPayload),
}

impl FindingEvent {
    /// Every kind this domain declares, as persisted.
    pub const KINDS: &'static [&'static str] = &[
        "finding_posted",
        "finding_updated",
        "finding_withdrawn",
        "finding_refused",
    ];

    /// The persisted `kind` string of this fact.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Posted(_) => "finding_posted",
            Self::Updated(_) => "finding_updated",
            Self::Withdrawn(_) => "finding_withdrawn",
            Self::Refused(_) => "finding_refused",
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
            Self::Posted(_) => false,
            Self::Updated(_) => false,
            Self::Withdrawn(_) => false,
            Self::Refused(_) => true,
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
