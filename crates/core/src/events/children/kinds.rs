//! Which facts this domain records.

use super::payloads::*;

/// A run this run started and how it closed, and the iterations of a loop —
/// one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum ChildEvent {
    LoopIteration(LoopIterationPayload),
    Created(ChildRunCreatedPayload),
    Finished(ChildRunFinishedPayload),
}
