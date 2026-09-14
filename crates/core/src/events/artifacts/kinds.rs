//! Which facts this domain records, and what each is called on the wire.

use super::payloads::*;

/// What a run holds: what a session handed over, what the engine — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactEvent {
    Written(ArtifactWrittenPayload),
    Submitted(ArtifactSubmittedPayload),
    Accepted(ArtifactAcceptedPayload),
}

impl ArtifactEvent {
    /// Every kind this domain declares, as persisted.
    pub const KINDS: &'static [&'static str] = &[
        "artifact_written",
        "artifact_submitted",
        "artifact_accepted",
    ];

    /// The persisted `kind` string of this fact.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Written(_) => "artifact_written",
            Self::Submitted(_) => "artifact_submitted",
            Self::Accepted(_) => "artifact_accepted",
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
            // A submission is the handover, and the acceptance that
            // follows is what the run holds: the ledger folds
            // acceptances, so nothing moves when a session hands
            // something over. What was refused is in the event and
            // nowhere else, which is exactly what audit means.
            Self::Written(_) => false,
            Self::Submitted(_) => true,
            Self::Accepted(_) => false,
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
