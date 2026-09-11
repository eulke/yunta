//! `yunta list --runs`: every local run as an inbox — grouped by what a
//! reader can do about it (what stopped on a person, what is still
//! moving, what has closed) and ordered inside each group by how long it
//! has been there, so the run to answer first is the first one read.
//!
//! The grouping and the line under each run are both read off the run's
//! own [`RunFrame`], through the same projection `yunta status` prints,
//! so the listing and the run's own page can never disagree about where
//! a run stands.

use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_core::events::StoredEvent;
use yunta_core::{Clock, Manifest, ModeName, RunId};
use yunta_engine::{RunFrame, RunPhase};
use yunta_storage::Storage;

use crate::commands::status::progress;
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::project::Project;
use crate::render::{cell_width, format_duration, indent, truncate, Glyphs, INDENT, LINE_WIDTH};

/// The cells a run id gets. A ULID is 26 characters, and the id is what
/// a reader copies into the next command, so this column pads a shorter
/// id and never cuts a longer one: a cut id is one nobody can use.
const ID_WIDTH: usize = 26;

/// The cells the workflow name and the run's mode share — enough for the
/// names a repo's own workflows carry, and the column the eye runs down
/// to find the run it came for.
const NAME_WIDTH: usize = 24;

/// The cells the time-in-state gets, right-aligned so a column of them
/// compares as numbers: `9s` through `23h59m` (see
/// [`crate::render::format_duration`]).
const AGE_WIDTH: usize = 6;

/// The part of a listing a run belongs in — the inbox's own grouping,
/// derived from the run's phase so a heading can never disagree with the
/// summary printed under it.
///
/// The order of the variants is the order the groups print in: what
/// stopped on a person comes before what is still moving, which comes
/// before what is already closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Standing {
    /// Stopped until a person acts: a decision to answer, or a log that
    /// stopped making sense.
    NeedsYou,
    /// Moving on its own, or created and not yet started.
    InFlight,
    /// Closed, however it closed.
    Closed,
}

impl Standing {
    /// Every group, in printing order.
    const ALL: [Standing; 3] = [Standing::NeedsYou, Standing::InFlight, Standing::Closed];

    /// Which group `phase` puts a run in. A log that stopped making sense
    /// needs a person as much as a decision does: nothing moves it on its
    /// own again.
    fn of(phase: &RunPhase) -> Self {
        match phase {
            RunPhase::Waiting { .. } | RunPhase::Broken { .. } => Standing::NeedsYou,
            RunPhase::Created | RunPhase::Running => Standing::InFlight,
            RunPhase::Finished
            | RunPhase::Failed { .. }
            | RunPhase::Cancelled
            | RunPhase::Promoted { .. } => Standing::Closed,
        }
    }

    /// The heading a group of runs prints under. It says what the reader
    /// can do about the rows below it, which is what a listing is read
    /// for.
    fn heading(self) -> &'static str {
        match self {
            Self::NeedsYou => "needs you",
            Self::InFlight => "in flight",
            Self::Closed => "closed",
        }
    }
}

/// One run as the listing shows it, read off that run's own frame.
struct RunRow {
    run_id: RunId,
    standing: Standing,
    workflow: String,
    mode: ModeName,
    /// How long the run has been where it is — what the rows of a group
    /// are ordered by.
    age: Duration,
    summary: String,
}

/// A run the listing can name but not derive: its log or the manifest it
/// froze does not read back. It is listed as itself, never dropped — a
/// run missing from a listing is a run nobody goes looking for.
struct Unreadable {
    run_id: RunId,
    problem: String,
}

pub fn list_runs() -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let storage = ctx.storage()?;

    let run_ids: Vec<RunId> = storage
        .list_runs()
        .map(|runs| runs.into_iter().map(|run| run.run_id).collect())?;
    if run_ids.is_empty() {
        println!("no runs in {}", ctx.project.storage_path.display());
        return Ok(Outcome::Success);
    }

    let now = ctx.clock.now();
    let mut rows = Vec::new();
    let mut unreadable = Vec::new();
    for run_id in run_ids {
        match run_row(&ctx.project, &storage, run_id, now) {
            Ok(row) => rows.push(row),
            Err(problem) => unreadable.push(problem),
        }
    }
    print!("{}", render_runs(rows, unreadable, Glyphs::from_env()));
    Ok(Outcome::Success)
}

/// The listing itself: what needs a person first, then what is still
/// moving, then what has closed, and last the runs that do not read
/// back. Within a group the run that has been there longest comes first
/// — the one that has been waiting since yesterday is the one to answer
/// before the one that stopped a minute ago — and runs that arrived at
/// their state together are ordered by id, which for a ULID is the order
/// they were created in.
///
/// A run under `unreadable` has no state to have been in for any length
/// of time, so that group is ordered by id alone.
fn render_runs(mut rows: Vec<RunRow>, mut unreadable: Vec<Unreadable>, glyphs: Glyphs) -> String {
    rows.sort_by(|a, b| b.age.cmp(&a.age).then_with(|| a.run_id.cmp(&b.run_id)));
    let mut out = String::new();
    for standing in Standing::ALL {
        let group: Vec<&RunRow> = rows.iter().filter(|row| row.standing == standing).collect();
        if group.is_empty() {
            continue;
        }
        push_heading(&mut out, standing.heading(), group.len());
        for row in group {
            out.push_str(&row.render(glyphs));
        }
    }
    if !unreadable.is_empty() {
        unreadable.sort_by(|a, b| a.run_id.cmp(&b.run_id));
        push_heading(&mut out, "unreadable", unreadable.len());
        for run in &unreadable {
            out.push_str(&format!("{INDENT}{}: {}\n", run.run_id, run.problem));
        }
    }
    out
}

/// A group's heading, with the runs in it counted: a reader who only
/// reads the headings still learns how much is waiting.
fn push_heading(out: &mut String, heading: &str, runs: usize) {
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!("{heading} ({runs})\n"));
}

impl RunRow {
    /// One row from the run's own frame: which group it belongs in, what
    /// the run is, and the line `yunta status` prints for it.
    fn of(frame: &RunFrame, age: Duration) -> Self {
        RunRow {
            run_id: frame.run_id.clone(),
            standing: Standing::of(&frame.phase),
            workflow: frame.workflow.clone(),
            mode: frame.mode.clone(),
            age,
            summary: progress::summary(frame),
        }
    }

    /// Two lines: what the run is, then where it stands. The identity
    /// line carries the id a reader copies, the workflow that names what
    /// the run is doing and the mode it does it in; the line under it is
    /// the same summary `yunta status` prints, so the two surfaces say
    /// the same thing about the same run.
    fn render(&self, glyphs: Glyphs) -> String {
        // The row hangs one step under the heading of its group, and
        // its summary one step further under the row, so the summary
        // reads as this run's line rather than the next run's.
        let margin = indent(2);
        format!(
            "{INDENT}{:<ID_WIDTH$}  {}  {:>AGE_WIDTH$}\n{margin}{}\n",
            self.run_id.as_str(),
            truncate(
                &format!("{} ({})", self.workflow, self.mode),
                NAME_WIDTH,
                glyphs
            ),
            format_duration(self.age),
            truncate(
                &self.summary,
                LINE_WIDTH.saturating_sub(cell_width(&margin)),
                glyphs
            )
            .trim_end(),
        )
    }
}

/// One run's row, or what stops it from having one.
fn run_row(
    project: &Project,
    storage: &Storage,
    run_id: RunId,
    now: DateTime<Utc>,
) -> Result<RunRow, Unreadable> {
    let events = storage
        .events_for_run(&run_id)
        .map_err(|e| Unreadable::new(&run_id, format!("its event log does not read back: {e}")))?;
    // The same frozen-path-aware search `status` uses, so a run created
    // under a since-changed `paths.runs` still lists.
    let run_dir = project.run_dir(run_id.as_str()).ok_or_else(|| {
        Unreadable::new(
            &run_id,
            format!(
                "manifest missing or unreadable under {}",
                project.runs_root.display()
            ),
        )
    })?;
    let manifest_path = run_dir.join("manifest.yaml");
    let manifest: Manifest = std::fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|text| yunta_core::yaml::parse(&text).ok())
        .ok_or_else(|| {
            Unreadable::new(
                &run_id,
                format!(
                    "manifest missing or unreadable at {}",
                    manifest_path.display()
                ),
            )
        })?;
    Ok(RunRow::of(
        &progress::frame(&run_id, &manifest, &events, now),
        time_in_state(&events, now),
    ))
}

impl Unreadable {
    fn new(run_id: &RunId, problem: String) -> Self {
        Unreadable {
            run_id: run_id.clone(),
            problem,
        }
    }
}

/// How long the run has been where it is: the time since its last event,
/// which is the event that put it there — a parked run's own
/// `run_paused`, a running node's last word, a closed run's
/// `run_finished`. Zero for a log with no events, and for one whose last
/// event is stamped after `now`, since a run cannot have been somewhere
/// for a negative time.
fn time_in_state(events: &[StoredEvent], now: DateTime<Utc>) -> Duration {
    events
        .last()
        .map(|event| (now - event.timestamp).to_std().unwrap_or(Duration::ZERO))
        .unwrap_or(Duration::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    use yunta_engine::WaitingOn;

    fn row(id: &'static str, standing: Standing, age_secs: u64) -> RunRow {
        RunRow {
            run_id: RunId::from_static(id),
            standing,
            workflow: "review".to_string(),
            mode: ModeName::default(),
            age: Duration::from_secs(age_secs),
            summary: "1/2 nodes · 0 reroutes · running".to_string(),
        }
    }

    /// The ids of the runs a listing names, in the order it names them —
    /// the first word of every line that opens with one, whether the line
    /// is a row's identity line or an unreadable run's `id: problem`.
    fn listed(text: &str) -> Vec<&str> {
        text.lines()
            .filter_map(|line| line.split_whitespace().next())
            .map(|word| word.trim_end_matches(':'))
            .filter(|word| word.len() == ID_WIDTH)
            .collect()
    }

    const OLDEST: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P1";
    const TIED_A: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P2";
    const TIED_B: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P3";
    const NEWEST: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P4";

    #[test]
    fn a_runs_phase_decides_the_group_it_is_listed_under() {
        // A log that stopped making sense needs a person as much as a
        // decision does: nothing moves it on its own again.
        for phase in [
            RunPhase::Waiting {
                on: WaitingOn::Run {
                    reason: "node `lint` failed".to_string(),
                },
            },
            RunPhase::Broken {
                diagnostic: "event 4 refers to a node the manifest does not declare".to_string(),
            },
        ] {
            assert_eq!(Standing::of(&phase), Standing::NeedsYou, "{phase:?}");
        }
        for phase in [RunPhase::Created, RunPhase::Running] {
            assert_eq!(Standing::of(&phase), Standing::InFlight, "{phase:?}");
        }
        for phase in [
            RunPhase::Finished,
            RunPhase::Failed { failure: None },
            RunPhase::Cancelled,
            RunPhase::Promoted { to: None },
        ] {
            assert_eq!(Standing::of(&phase), Standing::Closed, "{phase:?}");
        }
    }

    #[test]
    fn a_group_answers_the_run_that_has_waited_longest_first() {
        let text = render_runs(
            vec![
                row(NEWEST, Standing::NeedsYou, 30),
                row(OLDEST, Standing::NeedsYou, 86_400),
            ],
            Vec::new(),
            Glyphs::Ascii,
        );
        assert_eq!(listed(&text), [OLDEST, NEWEST], "{text}");
    }

    #[test]
    fn runs_that_arrived_together_are_ordered_by_id() {
        let text = render_runs(
            vec![
                row(TIED_B, Standing::InFlight, 60),
                row(TIED_A, Standing::InFlight, 60),
            ],
            Vec::new(),
            Glyphs::Ascii,
        );
        assert_eq!(listed(&text), [TIED_A, TIED_B], "{text}");
    }

    #[test]
    fn what_needs_a_person_is_listed_above_what_is_only_moving() {
        let text = render_runs(
            vec![
                row(NEWEST, Standing::Closed, 900),
                row(TIED_A, Standing::InFlight, 600),
                row(OLDEST, Standing::NeedsYou, 5),
            ],
            Vec::new(),
            Glyphs::Ascii,
        );
        assert_eq!(listed(&text), [OLDEST, TIED_A, NEWEST], "{text}");
        assert!(text.contains("needs you (1)"), "{text}");
        assert!(text.contains("in flight (1)"), "{text}");
        assert!(text.contains("closed (1)"), "{text}");
    }

    #[test]
    fn a_run_that_does_not_read_back_is_listed_last_and_by_id() {
        let text = render_runs(
            vec![row(OLDEST, Standing::Closed, 10)],
            vec![
                Unreadable::new(&RunId::from_static(NEWEST), "gone".to_string()),
                Unreadable::new(&RunId::from_static(TIED_A), "gone".to_string()),
            ],
            Glyphs::Ascii,
        );
        assert_eq!(listed(&text), [OLDEST, TIED_A, NEWEST], "{text}");
        assert!(text.contains("unreadable (2)"), "{text}");
    }
}
