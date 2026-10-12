//! Which facts this domain records.

use super::payloads::*;

/// A run this run started, and the iterations of the loop that started it — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum ChildEvent {
    LoopIteration(LoopIterationPayload),
    Created(ChildRunCreatedPayload),
    Finished(ChildRunFinishedPayload),
}
