//! Where a run stands, on the one line `yunta status` and
//! `yunta list --runs` both have room for.
//!
//! `yunta status` prints [`summary`] for one run and `yunta list --runs`
//! prints the same line under every row. A listing whose row said one
//! thing while the run's own page said another would be a listing nobody
//! could act on, so both project the same [`RunFrame`] — the engine's
//! derived snapshot, a pure function of the run's log, the workflow its
//! manifest froze and the instant it is read at.
//!
//! Counters with context, never percentages: a percentage lies the moment
//! a re-route grows the denominator.

use chrono::{DateTime, Utc};

use yunta_core::events::StoredEvent;
use yunta_core::{Manifest, RunId};
use yunta_engine::{Counter, RunFrame, RunPhase};

use crate::commands::advice;

/// Frames a run for the surfaces that say where it stands in one line.
///
/// `now` comes from the caller's injected clock: the frame measures the
/// run's wall-clock and every node's liveness against it, and nothing
/// under here reads a clock of its own.
///
/// The frame carries no prior estimation. Neither surface compares a run
/// against its workflow's history, and reading that history costs a pass
/// over every other run of the same workflow — once per row, for a
/// listing.
///
/// Framing a run costs what [`yunta_engine::run_frame`] says it costs.
/// `yunta status` pays that once, for the run it opens; `yunta list
/// --runs` pays it again for every run it prints.
pub(crate) fn frame(
    run_id: &RunId,
    manifest: &Manifest,
    events: &[StoredEvent],
    now: DateTime<Utc>,
) -> RunFrame {
    yunta_engine::run_frame(run_id, &manifest.workflow, events, None, now)
}

/// The counters and the phase on one line: two levels — flow (the DAG's
/// nodes, over what this run's mode schedules) and task (ledger tasks
/// done over registered) — each a counter with context.
pub(crate) fn summary(frame: &RunFrame) -> String {
    let mut summary = format!("{}/{} nodes", terminated(&frame.flow), frame.flow.total);
    if let Some(mode) = &frame.flow.skipped_by {
        summary.push_str(&format!(" · {} skipped (mode: {mode})", frame.flow.skipped));
    }
    if frame.flow.waiting > 0 {
        summary.push_str(&format!(" · {} waiting", frame.flow.waiting));
    }
    if let Some(tasks) = &frame.tasks {
        summary = format!("{}/{} tasks · {summary}", tasks.done, tasks.total);
    }
    summary.push_str(&format!(
        " · {} reroutes · {}",
        frame.reroutes,
        phase_label(&frame.phase)
    ));
    if let Some(note) = crate::commands::unknown_kinds_note(&frame.unknown_kinds) {
        summary.push_str(&format!(" · {note}"));
    }
    summary
}

/// The nodes this run has carried as far as they go, finished and failed
/// together: a line with room for one number says how much of the graph
/// is behind the run, and the phase beside it says whether the run
/// survived it.
fn terminated(flow: &Counter) -> usize {
    flow.done + flow.failed
}

/// The phase on one line, for the end of a summary.
fn phase_label(phase: &RunPhase) -> String {
    match phase {
        RunPhase::Created => "created".to_string(),
        RunPhase::Running => "running".to_string(),
        // A parked run is waiting on a person, not stuck, and what it is
        // parked on is the thing a reader acts on next.
        RunPhase::Waiting { on } => format!("waiting — {}", advice::parked_on(on)),
        // `finished` is the word every other surface reports a completed
        // run with; the other three are named as themselves, because a
        // run a person cancelled and a run that failed are not a run that
        // finished.
        RunPhase::Finished => "finished".to_string(),
        RunPhase::Failed { .. } => "failed".to_string(),
        RunPhase::Cancelled => "cancelled".to_string(),
        RunPhase::Promoted { .. } => "promoted".to_string(),
        RunPhase::Broken { diagnostic } => {
            format!("broken — {}", yunta_core::text::one_line(diagnostic))
        }
    }
}
