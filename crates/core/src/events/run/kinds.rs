//! Which facts this domain records, and what each is called on the wire.

use super::payloads::*;

/// The run itself: it is born, it parks, it wakes, it closes, and it — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum RunEvent {
    Created(RunCreatedPayload),
    PromotionSignaled(PromotionSignaledPayload),
    Paused(RunPausedPayload),
    Resumed(RunResumedPayload),
    Finished(RunFinishedPayload),
}

impl RunEvent {
    /// Every kind this domain declares, as persisted.
    pub const KINDS: &'static [&'static str] = &[
        "run_created",
        "promotion_signaled",
        "run_paused",
        "run_resumed",
        "run_finished",
    ];

    /// The persisted `kind` string of this fact.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Created(_) => "run_created",
            Self::PromotionSignaled(_) => "promotion_signaled",
            Self::Paused(_) => "run_paused",
            Self::Resumed(_) => "run_resumed",
            Self::Finished(_) => "run_finished",
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
