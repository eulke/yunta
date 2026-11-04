//! How a run stops for a person: the one `run_paused` every stop writes,
//! whatever asked for it — a cancellation, an exhausted budget, an
//! unanswered gate — and the pause a crash leaves for `yunta cancel` to
//! record.

use yunta_core::events::{EventPayload, PauseReason, RunEvent, RunPausedPayload};
use yunta_core::{Clock, Redactor, RunId};
use yunta_storage::AsyncStorage;

use crate::run_log::RunLog;

use super::{RunCtx, RunError, RunReport, RunTerminal};

/// Records the `run_paused` a post-crash `yunta cancel` writes when it
/// finds the engine already dead. The CLI never builds an `EventDraft`
/// itself: event construction and its clock stamp live here, so every
/// event on the log is emitted by the engine through one injected clock.
/// The run-level `run_paused` event — built in one place so every path
/// that stops a run (the scheduler's [`record_pause`], a crash's
/// [`record_pause_after_crash`]) writes the same event, which stores the
/// pause as typed data.
fn run_paused(reason: &PauseReason) -> EventPayload {
    EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(reason)))
}

/// Records the pause of a run whose engine is gone, with the redactor the
/// caller resolved from the run's manifest before it wrote anything.
pub async fn record_pause_after_crash(
    storage: &AsyncStorage,
    run_id: &RunId,
    reason: &PauseReason,
    clock: &dyn Clock,
    redactor: &Redactor,
) -> Result<(), RunError> {
    RunLog::new(storage, run_id, clock, redactor)
        .record(None, run_paused(reason))
        .await?;
    Ok(())
}

/// Records the run's pause, exports the forensic `events.jsonl`, and returns
/// the paused report — the one place a run stops for a human to resume,
/// whatever asked for it (a cancellation, an exhausted budget, an
/// unanswered gate).
pub(super) async fn pause(ctx: &RunCtx<'_>, reason: PauseReason) -> Result<RunReport, RunError> {
    record_pause(ctx, &reason).await?;
    Ok(RunReport {
        terminal: RunTerminal::Paused { reason },
        state: *ctx.run_view().await?.state,
    })
}

/// Emits the run-level `run_paused` and exports the forensic log — the
/// one place the pause event is written, shared by the scheduler's own
/// pause and the gate flow's.
async fn record_pause(ctx: &RunCtx<'_>, reason: &PauseReason) -> Result<(), RunError> {
    ctx.emit(None, run_paused(reason)).await?;
    ctx.export_events_jsonl().await
}
