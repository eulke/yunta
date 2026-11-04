//! Which facts this domain records.

use super::payloads::*;

/// What a run holds: what a session handed over, what the engine accepted
/// into the store, and what an older log says a node wrote — one variant
/// per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactEvent {
    Written(ArtifactWrittenPayload),
    Submitted(ArtifactSubmittedPayload),
    Accepted(ArtifactAcceptedPayload),
}
