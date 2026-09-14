//! Which facts this domain records, and what each is called on the wire.

use super::payloads::*;

/// A run this run started, and the iterations of the loop that started it — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum ChildEvent {
    LoopIteration(LoopIterationPayload),
    Created(ChildRunCreatedPayload),
    Finished(ChildRunFinishedPayload),
}

impl ChildEvent {
    /// Every kind this domain declares, as persisted.
    pub const KINDS: &'static [&'static str] =
        &["loop_iteration", "child_run_created", "child_run_finished"];

    /// The persisted `kind` string of this fact.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::LoopIteration(_) => "loop_iteration",
            Self::Created(_) => "child_run_created",
            Self::Finished(_) => "child_run_finished",
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
