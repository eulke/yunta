//! A person closing a run nobody is going to continue.
//!
//! A run stops on a person — a decision, a question, a budget — and
//! waits there for as long as it takes. When nobody will answer, the run
//! would wait forever; closing it records that a person ended it, and
//! who, so it stops asking.

use std::path::Path;

use yunta_core::events::{EventPayload, RunEvent, RunFinishedPayload, StoredEvent, TerminalState};
use yunta_core::{Clock, Redactor, Responder, RunId};
use yunta_storage::{AsyncStorage, StorageError};

use crate::run_log::RunLog;
use crate::EngineLiveness;

/// Why a run cannot be closed.
#[derive(Debug, thiserror::Error)]
pub enum CloseRunError {
    #[error("the run is already closed")]
    AlreadyClosed,
    #[error("an engine is driving the run")]
    Driven,
    /// Its log says it is moving and nothing says the engine that drove
    /// it is gone: it may be between two processes.
    #[error("the run has not stopped, and nothing says the engine that drove it is gone")]
    Moving,
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("could not write the run's events.jsonl: {0}")]
    Export(String),
}

/// Closes the run `run_id`, stopped and waiting on a person, as
/// `cancelled` by `by`, and writes its exported log under `run_dir`.
///
/// `engine` is what the run's registry says about the process driving
/// it: a run an engine is driving is that engine's to stop, never this
/// one's to close. A run whose engine died is settled first — what it
/// left running stopped and its pause recorded — so by the time this is
/// called it is stopped like any other.
///
/// Nothing else of the run is touched: its branch and its worktree stay
/// for a person to read, and for `gc` to remove. The closing event goes
/// through the run's log, redacted with `redactor`.
pub async fn close_run(
    storage: &AsyncStorage,
    run_id: &RunId,
    run_dir: &Path,
    clock: &dyn Clock,
    engine: EngineLiveness,
    by: Responder,
    redactor: &Redactor,
) -> Result<(), CloseRunError> {
    let events = storage.events_for_run(run_id.clone()).await?;
    if events.iter().any(finishes_the_run) {
        return Err(CloseRunError::AlreadyClosed);
    }
    if !events.last().is_some_and(pauses_the_run) {
        return Err(match engine {
            EngineLiveness::Alive => CloseRunError::Driven,
            EngineLiveness::Dead | EngineLiveness::Unrecorded | EngineLiveness::Unknown => {
                CloseRunError::Moving
            }
        });
    }
    let state = crate::replay::derive(&events);
    let closed = RunFinishedPayload::closed(
        TerminalState::Cancelled,
        state.total_tokens(),
        crate::stats::tasks_done(&state),
    )
    .closed_by(by);
    RunLog::new(storage, run_id, clock, redactor)
        .record(None, EventPayload::Run(RunEvent::Finished(closed)))
        .await?;
    let events = storage.events_for_run(run_id.clone()).await?;
    // blocking: `write_jsonl` replaces the whole file on this thread;
    // `tokio::fs` would run the same write and rename on a pool thread.
    crate::events_export::write_jsonl(run_dir, &events)
        .map_err(|error| CloseRunError::Export(yunta_core::describe(&error)))
}

fn finishes_the_run(event: &StoredEvent) -> bool {
    matches!(
        event.payload(),
        Some(EventPayload::Run(RunEvent::Finished(_)))
    )
}

fn pauses_the_run(event: &StoredEvent) -> bool {
    matches!(
        event.payload(),
        Some(EventPayload::Run(RunEvent::Paused(_)))
    )
}
