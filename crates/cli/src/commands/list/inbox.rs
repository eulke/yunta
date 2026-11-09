//! The runs a listing is about: this repository's runs, or every run
//! on the machine, each read off its own [`RunFrame`] through the same
//! projection `yunta status` prints, and grouped by what a reader can do
//! about it. The listing renders them; resolving `last` and `needs`
//! reads the same groups, so a run the listing shows under "needs you"
//! is the run `needs` names.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use yunta_core::{Clock, Manifest, RunId};
use yunta_render::page::{FieldGroup, FieldRow, RunUnread, UnreadableRun};
use yunta_replay::frame::{RunFrame, RunPhase};
use yunta_storage::Storage;

use crate::context::Context;
use crate::error::CliError;
use crate::project::Project;

/// The runs a listing is about, each read off its own frame: the runs
/// of the repository this is run in — every run on the machine with
/// `all`, or when this is run outside a repository — and how many runs
/// that leaves to other projects.
#[derive(serde::Serialize)]
pub(crate) struct Inbox {
    #[serde(serialize_with = "crate::json::field::rows")]
    pub(crate) rows: Vec<FieldRow>,
    pub(crate) unreadable: Vec<UnreadableRun>,
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
                        .is_none_or(|here| here.holds(&row.frame.run_id, project.as_deref())) =>
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
    pub(crate) fn needing_a_person(&self) -> Vec<&FieldRow> {
        let mut waiting: Vec<&FieldRow> = self
            .rows
            .iter()
            .filter(|row| row.group() == FieldGroup::NeedsYou)
            .collect();
        waiting.sort_by(|a, b| FieldRow::by_wait(a, b));
        waiting
    }

    /// The run of this listing created last — for a ULID, the greatest
    /// id.
    pub(crate) fn newest(&self) -> Option<&RunId> {
        self.rows
            .iter()
            .map(|row| &row.frame.run_id)
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

/// One run's row and the repository its manifest says it was created
/// in, or what stops it from having one.
fn run_row(
    project: &Project,
    storage: &Storage,
    run_id: RunId,
    now: DateTime<Utc>,
) -> Result<(FieldRow, Option<PathBuf>), UnreadableRun> {
    let events = storage
        .events_for_run(&run_id)
        .map_err(|e| UnreadableRun::new(&run_id, RunUnread::EventLog(e.to_string())))?;
    // The same frozen-path-aware search `status` uses, so a run created
    // under a since-changed `paths.runs` still lists.
    let run_dir = project.run_dir(run_id.as_str()).ok_or_else(|| {
        UnreadableRun::new(
            &run_id,
            RunUnread::NoDirectory {
                runs_root: project.runs_root.clone(),
            },
        )
    })?;
    let manifest_path = yunta_engine::run_dir::manifest_path(&run_dir);
    let manifest: Manifest = std::fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|text| yunta_core::yaml::parse(&text).ok())
        .ok_or_else(|| {
            UnreadableRun::new(
                &run_id,
                RunUnread::Manifest {
                    path: manifest_path.clone(),
                },
            )
        })?;
    let state = yunta_replay::RunState::replay(&events);
    let engine = yunta_engine::read_engine(&run_dir, &yunta_engine::lock::SystemProbe);
    let frame = RunFrame::of(&run_id, &manifest.workflow, &state, None, &engine, now);
    // Only a parked run has a menu to rebuild, and rebuilding one is a
    // walk of the run's state.
    let menu = matches!(frame.phase, RunPhase::Waiting { .. })
        && yunta_engine::awaits_decision(&manifest, &state).is_some();
    let row = FieldRow { frame, menu };
    Ok((row, manifest.project.map(|project| project.git_common_dir)))
}

/// Whether `ctx` is run inside a git repository: what decides whether a
/// listing is that repository's runs.
pub(crate) async fn in_a_repository(ctx: &Context) -> Result<bool, CliError> {
    Ok(Here::of(ctx).await?.is_some())
}
