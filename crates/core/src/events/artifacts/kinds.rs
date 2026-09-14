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

    /// The shape version of this kind. Every kind starts at 1 and a
    /// version is per kind, never per domain and never global: a kind
    /// that gains an incompatible shape becomes a new name, and only
    /// that one moves.
    pub fn schema_version(&self) -> u32 {
        1
    }
}
