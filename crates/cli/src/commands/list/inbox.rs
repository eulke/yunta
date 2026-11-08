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
use surco::words::Standing;
use yunta_core::step::NextStep;
use yunta_core::{Clock, Manifest, ModeName, RunId, WorkflowName};
use yunta_render::command;
use yunta_render::engine;
use yunta_render::pause;
use yunta_replay::frame::{RunFrame, RunPhase};
use yunta_storage::Storage;

use crate::commands::status::progress;
use crate::context::Context;
use crate::error::CliError;
use crate::project::Project;

/// The part of a listing a run belongs in — the inbox's own grouping,
/// derived from the run's phase so a heading can never disagree with the
/// summary printed under it.
///
/// The order of the variants is the order the groups print in: what
/// stopped on a person comes before what is still moving, which comes
/// before what is already closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Group {
    /// Stopped until a person acts: a decision to answer, or a log that
    /// stopped making sense.
    NeedsYou,
    /// Its log says it is moving and the engine that drove it is gone:
    /// it waits on a person as surely as a decision does, but what it
    /// takes is a resume, not an answer.
    Stalled,
    /// A person stopped it, and nothing is owed: a resume continues it
    /// whenever they choose.
    Stopped,
    /// Moving on its own, parked until a limit resets, or created and not
    /// yet started: the runs the listing calls `running`.
    Running,
    /// Closed, however it closed.
    Closed,
}

impl Group {
    /// Which group a run called `standing` belongs in, `closed` saying
    /// whether its log closed it. A log that stopped making sense needs a
    /// person as much as a decision does: nothing moves it on its own
    /// again.
    ///
    /// The grouping is read off the word every other surface calls the
    /// run by, so a heading and the summary under it are two views of
    /// one answer rather than two readings of a phase.
    fn of(standing: Standing, closed: bool) -> Self {
        match standing {
            _ if closed => Group::Closed,
            Standing::NeedsYou | Standing::Broken => Group::NeedsYou,
            Standing::Stalled => Group::Stalled,
            Standing::Stopped => Group::Stopped,
            Standing::Created | Standing::Running | Standing::Waiting => Group::Running,
            Standing::Finished
            | Standing::Reported
            | Standing::Failed
            | Standing::Rerouted
            | Standing::Promoted
            | Standing::Skipped
            | Standing::NeverRan => Group::Closed,
        }
    }
}

/// One run as the listing shows it, read off that run's own frame.
#[derive(Clone, serde::Serialize)]
pub(crate) struct RunRow {
    pub(crate) run_id: RunId,
    pub(crate) group: Group,
    /// What the run is called, as every surface calls it.
    pub(crate) word: Standing,
    pub(crate) workflow: WorkflowName,
    pub(crate) mode: ModeName,
    /// How long the run has been where it is — what the rows of a group
    /// are ordered by: the time since its last event, which is the event
    /// that put it there (a parked run's own `run_paused`, a running node's
    /// last word, a closed run's `run_finished`). Zero for a log with none.
    #[serde(rename = "age_secs", serialize_with = "as_secs")]
    pub(crate) age: Duration,
    /// What holds the run where it is, said whole; `None` for a run
    /// nothing holds.
    pub(crate) reason: Option<String>,
    /// The command that moves it on, when a person has one to run.
    pub(crate) command: Option<String>,
    /// What holds the run, on the one line a field's row has room for.
    pub(crate) said: String,
    /// Where each of its nodes stands, in declaration order.
    pub(crate) furrow: Vec<Standing>,
    /// Whether its engine is asking a person at a terminal now.
    pub(crate) asking: bool,
}

impl RunRow {
    /// The run as a field's row draws it, named `handle`, with `edge` at
    /// its far end.
    pub(crate) fn line(
        &self,
        handle: String,
        edge: surco::components::Edge,
    ) -> surco::components::RunLine {
        surco::components::RunLine {
            standing: self.word,
            handle,
            workflow: self.workflow.to_string(),
            mode: self.mode.to_string(),
            furrow: self.furrow.clone(),
            said: self.said.clone(),
            edge,
        }
    }
}

/// A run the listing can name but not derive: its log or the manifest it
/// froze does not read back. It is listed as itself, never dropped — a
/// run missing from a listing is a run nobody goes looking for.
#[derive(serde::Serialize)]
pub(crate) struct Unreadable {
    pub(crate) run_id: RunId,
    pub(crate) problem: String,
}

/// The runs a listing is about, each read off its own frame: the runs
/// of the repository this is run in — every run on the machine with
/// `all`, or when this is run outside a repository — and how many runs
/// that leaves to other projects.
#[derive(serde::Serialize)]
pub(crate) struct Inbox {
    pub(crate) rows: Vec<RunRow>,
    pub(crate) unreadable: Vec<Unreadable>,
    pub(crate) elsewhere: usize,
    /// Whether the listing is the runs of the repository it was asked in,
    /// rather than every run on the machine.
    #[serde(skip)]
    pub(crate) here: bool,
}

impl Inbox {
    /// Every run under `ctx`'s state root, sorted into the ones this
    /// listing is about and the count of the rest.
    pub(crate) async fn gather(ctx: &Context, all: bool) -> Result<Self, CliError> {
        let storage = ctx.storage()?;
        let run_ids: Vec<RunId> = storage
            .list_runs()
            .map(|runs| runs.into_iter().map(|run| run.run_id).collect())?;
        let here = match all {
            true => None,
            false => Here::of(ctx).await?,
        };
        let now = ctx.clock.now();
        let mut inbox = Inbox {
            rows: Vec::new(),
            unreadable: Vec::new(),
            elsewhere: 0,
            here: here.is_some(),
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

    /// The runs of this listing that wait on a person, the one that has
    /// waited longest first.
    pub(crate) fn needing_a_person(&self) -> Vec<&RunRow> {
        let mut waiting: Vec<&RunRow> = self
            .rows
            .iter()
            .filter(|row| row.group == Group::NeedsYou)
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
    /// run is listed, since there is no project to narrow to. Only git's own
    /// non-zero exit says so: a cancelled or unstartable git is an error.
    async fn of(ctx: &Context) -> Result<Option<Self>, CliError> {
        let common = yunta_engine::git::common_dir(&ctx.cwd, ctx.supervision()).await;
        let git_common_dir = match common {
            Ok(dir) => dir,
            Err(error) if matches!(error.cause, yunta_engine::git::Cause::Exit { .. }) => {
                return Ok(None);
            }
            Err(error) => return Err(yunta_engine::RunError::from(error).into()),
        };
        let branches = match yunta_engine::git::run_branches(&ctx.cwd, ctx.supervision()).await {
            Ok(branches) => branches,
            Err(error) if matches!(error.cause, yunta_engine::git::Cause::Exit { .. }) => {
                BTreeSet::new()
            }
            Err(error) => return Err(yunta_engine::RunError::from(error).into()),
        };
        Ok(Some(Here {
            git_common_dir,
            branches,
        }))
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
    /// the run is, how long it has stood there, what holds it and what moves
    /// it. `menu` says whether the run stopped on a decision whose options
    /// `yunta status` lists.
    fn of(frame: &RunFrame, menu: bool) -> Self {
        let word = frame.standing;
        let (reason, command) = held(frame, word, menu);
        let asking = word == Standing::NeedsYou
            && matches!(frame.phase, RunPhase::Created | RunPhase::Running)
            && frame.asking.is_some();
        let closed = matches!(
            frame.phase,
            RunPhase::Finished
                | RunPhase::Failed { .. }
                | RunPhase::Cancelled
                | RunPhase::Promoted { .. }
        );
        RunRow {
            run_id: frame.run_id.clone(),
            group: Group::of(word, closed),
            word,
            workflow: frame.workflow.clone(),
            mode: frame.mode.clone(),
            age: frame.since_last_event.unwrap_or_default(),
            reason,
            command,
            said: said(frame),
            furrow: frame.nodes.iter().map(|node| node.standing).collect(),
            asking,
        }
    }
}

/// What holds a run whose word is `word`, and the command that moves it:
/// `menu` says whether it stopped on a decision `yunta status` lists the
/// options of.
fn held(frame: &RunFrame, word: Standing, menu: bool) -> (Option<String>, Option<String>) {
    match (word, &frame.phase) {
        (Standing::Stalled, _) => (
            Some(engine::STALLED.to_string()),
            Some(command::line(&NextStep::Resume {
                run: frame.run_id.clone(),
            })),
        ),
        // Asked live, the question is answered where it is asked: no
        // command here reaches it.
        (Standing::NeedsYou, RunPhase::Created | RunPhase::Running) => {
            (frame.asking.as_ref().map(engine::asking_in_full), None)
        }
        (_, RunPhase::Waiting { on }) => (
            Some(pause::parked_in_full(on)),
            Some(match menu {
                true => command::line(&NextStep::status(&frame.run_id)),
                false => command::line(&NextStep::Resume {
                    run: frame.run_id.clone(),
                }),
            }),
        ),
        (_, RunPhase::Broken { diagnostic }) => (
            Some(surco::text::one_line(diagnostic)),
            Some(command::line(&NextStep::Verify {
                run: frame.run_id.clone(),
            })),
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
        ) => (Some(yunta_render::failure::headline(failure)), None),
        (
            _,
            RunPhase::Finished
            | RunPhase::Failed { failure: None }
            | RunPhase::Cancelled
            | RunPhase::Promoted { .. },
        ) => (None, None),
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
    let state = yunta_replay::RunState::replay(&events);
    let engine = yunta_engine::read_engine(&run_dir, &yunta_engine::lock::SystemProbe);
    let frame = progress::frame_of(&run_id, &manifest, &state, &engine, now);
    // Only a parked run has a menu to rebuild, and rebuilding one is a
    // walk of the run's state.
    let menu = matches!(frame.phase, RunPhase::Waiting { .. })
        && yunta_engine::awaits_decision(&manifest, &state).is_some();
    let row = RunRow::of(&frame, menu);
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

/// What holds a run, on the one line a field's row has room for: the node
/// a live engine asks about, the claim a parked run stopped on, the stall,
/// how far a moving run has come, what failed a closed one. Nothing for a
/// run that finished.
fn said(frame: &RunFrame) -> String {
    match (frame.standing, &frame.phase) {
        (Standing::Stalled, _) => engine::STALLED.to_string(),
        (Standing::NeedsYou, RunPhase::Created | RunPhase::Running) => frame
            .asking
            .as_ref()
            .map(engine::asking_on)
            .unwrap_or_default(),
        (_, RunPhase::Waiting { on }) => pause::parked_claim(on),
        (_, RunPhase::Broken { diagnostic }) => surco::text::one_line(diagnostic),
        (_, RunPhase::Created | RunPhase::Running) => {
            crate::render::counter::progress(frame, crate::render::glyphs())
        }
        (
            _,
            RunPhase::Failed {
                failure: Some(failure),
            },
        ) => yunta_render::failure::headline(failure),
        (
            _,
            RunPhase::Finished
            | RunPhase::Failed { failure: None }
            | RunPhase::Cancelled
            | RunPhase::Promoted { .. },
        ) => String::new(),
    }
}

/// Whether `ctx` is run inside a git repository: what decides whether a
/// listing is that repository's runs.
pub(crate) async fn in_a_repository(ctx: &Context) -> Result<bool, CliError> {
    Ok(Here::of(ctx).await?.is_some())
}

/// A run's age in seconds, as a JSON reader compares it.
fn as_secs<S: serde::Serializer>(age: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64(age.as_secs_f64())
}

#[cfg(test)]
mod tests;
