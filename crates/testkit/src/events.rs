//! Reading a run's log the way a test asks about it.

use yunta_core::events::artifacts::ArtifactRef;
use yunta_core::events::{EventPayload, StoredEvent};

/// Every `artifact_accepted` on `events`, in log order — one entry per
/// event, never folded, so a test sees an identity accepted twice as two
/// acceptances and can say where each sits in the log.
pub fn accepted(events: &[StoredEvent]) -> Vec<ArtifactRef> {
    events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::ArtifactAccepted(p)) => Some(ArtifactRef {
                producer: event.node_id.clone(),
                artifact: p.artifact.clone(),
                content_hash: p.content_hash.clone(),
                origin: p.origin.clone(),
                seq: event.seq,
            }),
            _ => None,
        })
        .collect()
}
