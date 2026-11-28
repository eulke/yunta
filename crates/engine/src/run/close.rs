//! A person closing a run nobody is going to continue.
//!
//! A run stops on a person — a decision, a question, a budget — and
//! waits there for as long as it takes. When nobody will answer, the run
//! would wait forever; closing it records that a person ended it, and
//! who, so it stops asking.

use std::path::Path;

use yunta_core::events::{EventPayload, RunEvent, RunFinishedPayload, TerminalState};
use yunta_core::refusal::CloseRefusal;
use yunta_core::{Clock, Redactor, Responder, RunId};
use yunta_storage::{AsyncStorage, StorageError};

use crate::run_log::RunLog;
use yunta_replay::registry::Engine;
use yunta_replay::RunState;

/// Why a run is not closed: a refusal, or what failed below.
#[derive(Debug, thiserror::Error)]
pub enum CloseRunError {
    /// What the run makes Yunta refuse.
    #[error(transparent)]
    Refused(#[from] CloseRefusal),
    #[error("storage")]
    Storage(#[from] StorageError),
    /// The run's exported log could not be written; `0` says why.
    #[error("export")]
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
/// Nothing else of the run is touched here: its branch stays for a person
/// to read, and `gc` never removes it; its checkouts are the caller's to
/// give back to the project's pool. The closing event goes through the
/// run's log, redacted with `redactor`.
pub async fn close_run(
    storage: &AsyncStorage,
    run_id: &RunId,
    run_dir: &Path,
    clock: &dyn Clock,
    engine: &Engine,
    by: Responder,
    redactor: &Redactor,
) -> Result<(), CloseRunError> {
    let state = crate::run_log::RunView::of(storage.events_for_run(run_id.clone()).await?).state;
    if let Some(refusal) = close_refusal(&state, engine) {
        return Err(CloseRunError::Refused(refusal));
    }
    let closed =
        RunFinishedPayload::closed(TerminalState::Cancelled, state.metrics()).closed_by(by);
    RunLog::new(storage, run_id, clock, redactor)
        .record(None, EventPayload::Run(RunEvent::Finished(closed)))
        .await?;
    let events = storage.events_for_run(run_id.clone()).await?;
    // blocking: `write_jsonl` replaces the whole file on this thread;
    // `tokio::fs` would run the same write and rename on a pool thread.
    crate::events_export::write_jsonl(run_dir, &events)
        .map_err(|error| CloseRunError::Export(yunta_core::describe(&error)))
}

/// Why a close refuses the run `state` derives, `engine` what its registry says drives it:
/// its log says it closed already; or it has not stopped, and an engine that lives drives it,
/// or nothing says the engine that drove it is gone. None for a run stopped on a person, which
/// a close takes. The one rule [`close_run`] refuses by, which a reader that keeps a run's
/// state asks to say why a close leaves the run open.
pub fn close_refusal(state: &RunState, engine: &Engine) -> Option<CloseRefusal> {
    if state.run.finished().is_some() {
        return Some(CloseRefusal::AlreadyClosed);
    }
    if state.tail_paused() {
        return None;
    }
    Some(match engine {
        Engine::Alive { .. } => CloseRefusal::Driven,
        Engine::Dead | Engine::Unrecorded | Engine::Unknown => CloseRefusal::Moving,
    })
}
