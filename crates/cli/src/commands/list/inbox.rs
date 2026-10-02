//! The runs a listing is about: this repository's runs, or every run
//! on the machine, each read off its own [`RunFrame`] through the same
//! projection `yunta status` prints, and grouped by what a reader can do
//! about it. The listing renders them; resolving `last` and `needs`
//! reads the same groups, so a run the listing shows under "needs you"
//! is the run `needs` names.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_core::events::StoredEvent;
use yunta_core::{Clock, Manifest, ModeName, RunId, WorkflowName};
use yunta_engine::{EngineLiveness, RunFrame, RunPhase};
use yunta_storage::Storage;

use crate::commands::advice;
use crate::commands::status::progress;
use crate::context::Context;
use crate::error::CliError;
use crate::project::Project;
use crate::render::state::RunWord;

/// The part of a listing a run belongs in — the inbox's own grouping,
/// derived from the run's phase so a heading can never disagree with the
/// summary printed under it.
///
/// The order of the variants is the order the groups print in: what
/// stopped on a person comes before what is still moving, which comes
/// before what is already closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Standing {
    /// Stopped until a person acts: a decision to answer, or a log that
    /// stopped making sense.
    NeedsYou,
    /// Its log says it is moving and the engine that drove it is gone:
    /// it waits on a person as surely as a decision does, but what it
    /// takes is a resume, not an answer.
    Stalled,
    /// Moving on its own, or created and not yet started.
    InFlight,
    /// Closed, however it closed.
    Closed,
}

impl Standing {
    /// Every group, in printing order.
    pub(crate) const ALL: [Standing; 4] = [
        Standing::NeedsYou,
        Standing::Stalled,
        Standing::InFlight,
        Standing::Closed,
    ];

    /// Which group a run called `word` belongs in. A log that stopped
    /// making sense needs a person as much as a decision does: nothing
    /// moves it on its own again.
    ///
    /// The grouping is read off the word every other surface calls the
    /// run by, so a heading and the summary under it are two views of
    /// one answer rather than two readings of a phase.
    fn of(word: RunWord) -> Self {
        match word {
            RunWord::NeedsYou | RunWord::Broken => Standing::NeedsYou,
            RunWord::Stalled => Standing::Stalled,
            RunWord::Created | RunWord::Running => Standing::InFlight,
            RunWord::Finished
            | RunWord::Reported
            | RunWord::Failed
            | RunWord::Cancelled
            | RunWord::Promoted => Standing::Closed,
        }
    }

    /// The heading a group of runs prints under. It says what the reader
    /// can do about the rows below it, which is what a listing is read
    /// for.
    pub(crate) fn heading(self) -> &'static str {
        match self {
            Self::NeedsYou => "needs you",
            Self::Stalled => "stalled",
            Self::InFlight => "running",
            Self::Closed => "closed",
        }
    }
}

/// One run as the listing shows it, read off that run's own frame.
pub(crate) struct RunRow {
    pub(crate) run_id: RunId,
    pub(crate) standing: Standing,
    /// What the run is called, as every surface calls it.
    pub(crate) word: RunWord,
    pub(crate) workflow: WorkflowName,
    pub(crate) mode: ModeName,
    /// How long the run has been where it is — what the rows of a group
    /// are ordered by.
    pub(crate) age: Duration,
    /// What holds the run where it is, said whole; `None` for a run
    /// nothing holds.
    pub(crate) reason: Option<String>,
    /// The command that moves it on, when a person has one to run.
    pub(crate) command: Option<String>,
}

/// A run the listing can name but not derive: its log or the manifest it
/// froze does not read back. It is listed as itself, never dropped — a
/// run missing from a listing is a run nobody goes looking for.
pub(crate) struct Unreadable {
    pub(crate) run_id: RunId,
    pub(crate) problem: String,
}

/// The runs a listing is about, each read off its own frame: the runs
/// of the repository this is run in — every run on the machine with
/// `all`, or when this is run outside a repository — and how many runs
/// that leaves to other projects.
pub(crate) struct Inbox {
    pub(crate) rows: Vec<RunRow>,
    pub(crate) unreadable: Vec<Unreadable>,
    pub(crate) elsewhere: usize,
}

impl Inbox {
    /// Every run under `ctx`'s state root, sorted into the ones this
    /// listing is about and the count of the rest.
    pub(crate) async fn gather(ctx: &Context, all: bool) -> Result<Self, CliError> {
        let storage = ctx.storage()?;
        let run_ids: Vec<RunId> = storage
            .list_runs()
            .map(|runs| runs.into_iter().map(|run| run.run_id).collect())?;
        let here = match all || run_ids.is_empty() {
            true => None,
            false => Here::of(ctx).await,
        };
        let now = ctx.clock.now();
        let mut inbox = Inbox {
            rows: Vec::new(),
            unreadable: Vec::new(),
            elsewhere: 0,
        };
        for run_id in run_ids {
            match run_row(&ctx.project, &storage, run_id, now) {
                Ok((row, project))
                    if here
                        .as_ref()
                        .is_none_or(|here| here.holds(&row.run_id, project.as_deref())) =>
                {
                    inbox.rows.push(row)
                }
                Err(problem)
                    if here
                        .as_ref()
                        .is_none_or(|here| here.holds(&problem.run_id, None)) =>
                {
                    inbox.unreadable.push(problem)
                }
                Ok(_) | Err(_) => inbox.elsewhere += 1,
            }
        }
        Ok(inbox)
    }

    /// Whether no run under the state root reads as anything at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.unreadable.is_empty() && self.elsewhere == 0
    }

    /// The runs of this listing that wait on a person, the one that has
    /// waited longest first.
    pub(crate) fn needing_a_person(&self) -> Vec<&RunRow> {
        let mut waiting: Vec<&RunRow> = self
            .rows
            .iter()
            .filter(|row| row.standing == Standing::NeedsYou)
            .collect();
        waiting.sort_by(|a, b| b.age.cmp(&a.age).then_with(|| a.run_id.cmp(&b.run_id)));
        waiting
    }

    /// The run of this listing created last — for a ULID, the greatest
    /// id.
    pub(crate) fn newest(&self) -> Option<&RunId> {
        self.rows
            .iter()
            .map(|row| &row.run_id)
            .chain(self.unreadable.iter().map(|run| &run.run_id))
            .max()
    }
}

/// The repository a listing is asked in: the git directory its checkouts
/// share, and the runs that have their own branch in it.
struct Here {
    git_common_dir: PathBuf,
    branches: BTreeSet<RunId>,
}

impl Here {
    /// The repository `ctx.cwd` is in, or `None` outside one — where every
    /// run is listed, since there is no project to narrow to.
    async fn of(ctx: &Context) -> Option<Self> {
        let git_common_dir = yunta_engine::git::common_dir(&ctx.cwd, ctx.supervision())
            .await
            .ok()?;
        let branches = yunta_engine::git::run_branches(&ctx.cwd, ctx.supervision())
            .await
            .unwrap_or_default();
        Some(Here {
            git_common_dir,
            branches,
        })
    }

    /// Whether the run `run_id`, created in the repository whose git
    /// directory is `project` when its manifest says so, belongs here. A
    /// run that does not say is placed by its branch.
    fn holds(&self, run_id: &RunId, project: Option<&Path>) -> bool {
        match project {
            Some(project) => project == self.git_common_dir,
            None => self.branches.contains(run_id),
        }
    }
}

impl RunRow {
    /// One row from the run's own frame: which group it belongs in, what
    /// the run is, what holds it and what moves it. `menu` says whether
    /// the run stopped on a decision whose options `yunta status` lists.
    fn of(frame: &RunFrame, engine: EngineLiveness, age: Duration, menu: bool) -> Self {
        let word = crate::render::observed_word(frame, engine);
        let handle = frame.run_id.handle();
        let (reason, command) = match (word, &frame.phase) {
            (RunWord::Stalled, _) => (
                Some(advice::STALLED.to_string()),
                Some(advice::resume(handle)),
            ),
            (_, RunPhase::Waiting { on }) => (
                Some(advice::parked_in_full(on)),
                Some(match menu {
                    true => advice::status(handle),
                    false => advice::resume(handle),
                }),
            ),
            (_, RunPhase::Broken { diagnostic }) => (
                Some(yunta_core::text::one_line(diagnostic)),
                Some(advice::verify(handle)),
            ),
            (_, RunPhase::Created | RunPhase::Running) => (
                Some(crate::render::counter::line(frame, crate::render::glyphs())),
                None,
            ),
            (
                _,
                RunPhase::Failed {
                    failure: Some(failure),
                },
            ) => (Some(failure.headline()), None),
            (
                _,
                RunPhase::Finished
                | RunPhase::Failed { failure: None }
                | RunPhase::Cancelled
                | RunPhase::Promoted { .. },
            ) => (None, None),
        };
        RunRow {
            run_id: frame.run_id.clone(),
            standing: Standing::of(word),
            word,
            workflow: frame.workflow.clone(),
            mode: frame.mode.clone(),
            age,
            reason,
            command,
        }
    }
}

/// One run's row and the repository its manifest says it was created
/// in, or what stops it from having one.
fn run_row(
    project: &Project,
    storage: &Storage,
    run_id: RunId,
    now: DateTime<Utc>,
) -> Result<(RunRow, Option<PathBuf>), Unreadable> {
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
    let manifest_path = yunta_engine::run_dir::manifest_path(&run_dir);
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
    let frame = progress::frame(&run_id, &manifest, &events, now);
    // Only a parked run has a menu to rebuild, and rebuilding one is a
    // walk of the log.
    let menu = matches!(frame.phase, RunPhase::Waiting { .. })
        && yunta_engine::awaits_decision(&manifest, &yunta_engine::derive(&events)).is_some();
    let row = RunRow::of(
        &frame,
        yunta_engine::engine_liveness(&run_dir, &yunta_engine::lock::SystemProbe),
        time_in_state(&events, now),
        menu,
    );
    Ok((row, manifest.project.map(|project| project.git_common_dir)))
}

impl Unreadable {
    pub(crate) fn new(run_id: &RunId, problem: String) -> Self {
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

    #[test]
    fn the_word_a_run_is_called_by_decides_the_group_it_is_listed_under() {
        // A log that stopped making sense needs a person as much as a
        // decision does: nothing moves it on its own again.
        for word in [RunWord::NeedsYou, RunWord::Broken] {
            assert_eq!(Standing::of(word), Standing::NeedsYou, "{word}");
        }
        for word in [RunWord::Created, RunWord::Running] {
            assert_eq!(Standing::of(word), Standing::InFlight, "{word}");
        }
        for word in [
            RunWord::Finished,
            RunWord::Reported,
            RunWord::Failed,
            RunWord::Cancelled,
            RunWord::Promoted,
        ] {
            assert_eq!(Standing::of(word), Standing::Closed, "{word}");
        }
    }

    #[test]
    fn every_word_a_run_can_be_called_by_has_a_group() {
        // The listing is an inbox: a run whose word fell through would
        // be a run nobody goes looking for.
        let groups: Vec<Standing> = RunWord::ALL.into_iter().map(Standing::of).collect();
        assert_eq!(groups.len(), RunWord::ALL.len());
    }
}
