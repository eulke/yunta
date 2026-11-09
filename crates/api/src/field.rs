//! See [`crate`]. The field: the runs a project's listing is about — the
//! runs of the repository it is rooted in, or every run on this machine —
//! each read off its own [`RunFrame`] through the same projection `yunta
//! status` prints, and grouped by what a reader can do about it. Naming a
//! run `last` or `needs` reads the same groups, so a run the field shows
//! under "needs you" is the run `needs` names.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use yunta_core::{Manifest, RunId};
use yunta_engine::git::Cause;
use yunta_render::page::{FieldGroup, FieldRow, RunUnread, UnreadableRun};
use yunta_replay::frame::{RunFrame, RunPhase};
use yunta_replay::RunState;
use yunta_storage::Storage;

use crate::{Error, Failure, Project, Stop};

/// Which runs a field is about.
///
/// Not core's `step::Reach`, the reach a command line names, which says how
/// deep a listing reads as well.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldReach {
    /// The runs of the repository the project is rooted in; every run on
    /// this machine outside one, where there is no repository to narrow to.
    Here,
    /// Every run on this machine.
    Machine,
}

/// The runs a field is about, each read off its own frame, and how many
/// runs that leaves to other projects.
///
/// Not render's field page, which a person reads drawn from it.
pub struct FieldReading {
    pub rows: Vec<FieldRow>,
    /// The runs it can name but not read.
    pub unreadable: Vec<UnreadableRun>,
    /// How many runs it leaves to other projects.
    pub elsewhere: usize,
    /// Whether it is the runs of the repository it was asked in, rather
    /// than every run on this machine.
    pub here: bool,
}

impl Project {
    /// The field `reach` asks for: every run this project's store holds,
    /// sorted into the ones the field is about and the count of the rest.
    pub async fn field(&self, reach: FieldReach, stop: &Stop) -> Result<FieldReading, Error> {
        let storage = Storage::open(&self.storage_path).map_err(Failure::new)?;
        let run_ids: Vec<RunId> = storage
            .list_runs()
            .map_err(Failure::new)?
            .into_iter()
            .map(|run| run.run_id)
            .collect();
        let here = match reach {
            FieldReach::Machine => None,
            FieldReach::Here => Here::of(self, stop).await?,
        };
        let now = self.workspace.clock.now();
        let mut field = FieldReading {
            rows: Vec::new(),
            unreadable: Vec::new(),
            elsewhere: 0,
            here: here.is_some(),
        };
        for run_id in run_ids {
            match run_row(self, &storage, run_id, now) {
                Ok((row, project))
                    if here
                        .as_ref()
                        .is_none_or(|here| here.holds(&row.frame.run_id, project.as_deref())) =>
                {
                    field.rows.push(row)
                }
                Err(problem)
                    if here
                        .as_ref()
                        .is_none_or(|here| here.holds(&problem.run_id, None)) =>
                {
                    field.unreadable.push(problem)
                }
                Ok(_) | Err(_) => field.elsewhere += 1,
            }
        }
        Ok(field)
    }

    /// Whether this project is rooted inside a git repository: what decides
    /// whether its field is that repository's runs.
    pub async fn in_a_repository(&self, stop: &Stop) -> Result<bool, Error> {
        Ok(Here::of(self, stop).await?.is_some())
    }
}

impl FieldReading {
    /// The runs of this field that wait on a person, the one that has
    /// waited longest first.
    pub fn needing_a_person(&self) -> Vec<&FieldRow> {
        let mut waiting: Vec<&FieldRow> = self
            .rows
            .iter()
            .filter(|row| row.group() == FieldGroup::NeedsYou)
            .collect();
        waiting.sort_by(|a, b| FieldRow::by_wait(a, b));
        waiting
    }

    /// The run of this field created last — for a ULID, the greatest id.
    pub fn newest(&self) -> Option<&RunId> {
        self.rows
            .iter()
            .map(|row| &row.frame.run_id)
            .chain(self.unreadable.iter().map(|run| &run.run_id))
            .max()
    }
}

/// The repository a field is asked in: the git directory its checkouts
/// share, and the runs that have their own branch in it.
struct Here {
    git_common_dir: PathBuf,
    branches: BTreeSet<RunId>,
}

impl Here {
    /// The repository `project` is rooted in, or `None` outside one — where
    /// every run is listed, since there is no project to narrow to. Only
    /// git's own non-zero exit says so: a stopped or unstartable git is not
    /// an answer.
    async fn of(project: &Project, stop: &Stop) -> Result<Option<Self>, Error> {
        let workspace = &project.workspace;
        let common = yunta_engine::git::common_dir(&workspace.dir, workspace.supervision(stop));
        let git_common_dir = match common.await {
            Ok(dir) => dir,
            Err(error) if matches!(error.cause, Cause::Exit { .. }) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let branches = yunta_engine::git::run_branches(&workspace.dir, workspace.supervision(stop));
        let branches = match branches.await {
            Ok(branches) => branches,
            Err(error) if matches!(error.cause, Cause::Exit { .. }) => BTreeSet::new(),
            Err(error) => return Err(error.into()),
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

/// Whether the run `frame` draws stopped on a decision with a menu, which
/// `yunta status` lists the options of and a row offers the command that
/// answers: only a parked run has one to rebuild, and rebuilding one is a walk
/// of its `state` under the workflow its `manifest` froze.
pub fn offers_a_menu(frame: &RunFrame, manifest: &Manifest, state: &RunState) -> bool {
    matches!(frame.phase, RunPhase::Waiting { .. })
        && yunta_engine::awaits_decision(manifest, state).is_some()
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
    let manifest: Manifest = std::fs::read(&manifest_path)
        .ok()
        .and_then(|bytes| Manifest::read_frozen(&bytes).ok())
        .map(|frozen| frozen.doc)
        .ok_or_else(|| {
            UnreadableRun::new(
                &run_id,
                RunUnread::Manifest {
                    path: manifest_path.clone(),
                },
            )
        })?;
    let state = yunta_replay::RunState::replay(&events);
    let engine = crate::RunRegistry::at(run_dir).engine();
    let frame = RunFrame::of(&run_id, &manifest.workflow, &state, None, &engine, now);
    let menu = offers_a_menu(&frame, &manifest, &state);
    let row = FieldRow { frame, menu };
    Ok((row, manifest.project.map(|project| project.git_common_dir)))
}
