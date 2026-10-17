//! Which facts this domain records.

use super::payloads::*;

/// What a session reported about work that is not its own to fix —
/// posted, updated, withdrawn, refused, answered by another node, proved
/// by that node's work, or settled by a person: one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum FindingEvent {
    Posted(FindingPostedPayload),
    Updated(FindingUpdatedPayload),
    Withdrawn(FindingWithdrawnPayload),
    Refused(FindingRefusedPayload),
    Answered(FindingAnsweredPayload),
    Proved(FindingProvedPayload),
    Settled(FindingSettledPayload),
}
