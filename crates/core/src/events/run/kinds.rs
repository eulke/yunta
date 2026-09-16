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
    /// What the suite the run's lineage declared did on the tree the
    /// run opens on — measured by this run on its first wake, or held
    /// from birth because a run of the same lineage measured it.
    BaselineCaptured(BaselineCapturedPayload),
}

impl RunEvent {
    /// Every kind this domain declares, as persisted.
    pub const KINDS: &'static [&'static str] = &[
        "run_created",
        "baseline_captured",
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
            Self::BaselineCaptured(_) => "baseline_captured",
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
            Self::Created(_) => false,
            Self::PromotionSignaled(_) => false,
            Self::Paused(_) => false,
            Self::Resumed(_) => false,
            Self::Finished(_) => false,
            Self::BaselineCaptured(_) => false,
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
