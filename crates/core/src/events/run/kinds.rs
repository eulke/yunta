//! Which facts this domain records.

use super::payloads::*;

/// The run itself: it is born, it parks, it wakes, it moves, it closes, and
/// it asks to be run wider than its mode — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum RunEvent {
    Created(RunCreatedPayload),
    PromotionSignaled(PromotionSignaledPayload),
    Paused(RunPausedPayload),
    Resumed(RunResumedPayload),
    /// The run moved into a checkout that already built where it stands.
    Moved(RunMovedPayload),
    Finished(RunFinishedPayload),
    /// What the suite the run's lineage declared did on the tree the
    /// run opens on — measured by this run on its first wake, or held
    /// from birth because a run of the same lineage measured it.
    BaselineCaptured(BaselineCapturedPayload),
    /// What a command `baseline.held` names did on the same tree as the
    /// suite: measured beside it, or held from birth.
    BaselineHeld(BaselineHeldPayload),
    /// The host was suspended while the run was open: a fact about the
    /// machine, not something the run did.
    HostSuspended(HostSuspendedPayload),
}
