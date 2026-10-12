//! Which facts this domain records.

use super::payloads::*;

/// What a run holds: what a session handed over, what the engine — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactEvent {
    Written(ArtifactWrittenPayload),
    Submitted(ArtifactSubmittedPayload),
    Accepted(ArtifactAcceptedPayload),
}
