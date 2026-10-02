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

use crate::render::state::RunWord;
use crate::render::Glyphs;
use chrono::{DateTime, Utc};

use yunta_core::events::StoredEvent;
use yunta_core::{Manifest, RunId};
use yunta_engine::{EngineLiveness, RunFrame, RunPhase};

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

/// The counters and the phase on one line: the counters every surface
/// prints for the run, then the word the run is called by and what
/// qualifies it.
///
/// `engine` is what the run's registry says about the process driving
/// it, which is what tells a run that is moving from one whose engine is
/// gone.
pub(crate) fn summary(frame: &RunFrame, engine: EngineLiveness, glyphs: Glyphs) -> String {
    let sep = glyphs.sep();
    let mut summary = format!(
        "{} {sep} {}",
        crate::render::counter::line(frame, glyphs),
        phase_label(frame, engine)
    );
    if let Some(note) = crate::commands::unknown_kinds_note(&frame.unknown_kinds) {
        summary.push_str(&format!(" {sep} {note}"));
    }
    summary
}

/// The phase on one line, for the end of a summary: the word every
/// surface calls it by, and what qualifies it when something does.
fn phase_label(frame: &RunFrame, engine: EngineLiveness) -> String {
    let word = crate::render::observed_word(frame, engine);
    if word == RunWord::Stalled {
        return format!("{word} — no process is driving it");
    }
    match &frame.phase {
        // A parked run is waiting on a person, not stuck, and what it is
        // parked on is the thing a reader acts on next.
        RunPhase::Waiting { on } => format!("{word} — {}", advice::parked_on(on)),
        RunPhase::Broken { diagnostic } => {
            format!("{word} — {}", yunta_core::text::one_line(diagnostic))
        }
        RunPhase::Created
        | RunPhase::Running
        | RunPhase::Finished
        | RunPhase::Failed { .. }
        | RunPhase::Cancelled
        | RunPhase::Promoted { .. } => word.to_string(),
    }
}
