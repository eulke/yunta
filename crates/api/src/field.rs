//! See [`crate`]. The field: the runs a project's listing is about — the
//! runs of the repository it is rooted in, or every run on this machine —
//! each read off its own [`RunFrame`](yunta_replay::frame::RunFrame) through
//! the same projection `yunta status` prints, and grouped by what a reader can
//! do about it. Naming a run `last` or `needs` reads the same groups, so a run
//! the field shows under "needs you" is the run `needs` names.
//!
//! A run is read for a field one way, for a listing and a following alike
//! ([`Placed::of`]): its directory, the manifest it froze and, only when that
//! manifest places it here, its log, folded into a [`FollowedRun`] whose row
//! is [`FollowedRun::row`]. So a run of another repository costs no read of
//! the log, and a run's row reads the same on every interface. The run a
//! following reads whole is read the same way, wherever it stands
//! ([`read_whole`]).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use yunta_core::events::StoredEvent;
use yunta_core::{Manifest, RunId};
use yunta_engine::git::Cause;
use yunta_render::page::{FieldGroup, FieldRow, RunUnread, UnreadableRun};
use yunta_storage::LogReader;

use crate::run::read_manifest;
use crate::{Error, Failure, FollowedRun, Project, RunRegistry, Stop};

/// Which runs a field is about.
///
/// Not core's `step::Reach`, the reach a command line names, which says how
/// deep a listing reads as well.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone)]
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
    /// sorted into the ones the field is about and the count of the rest,
    /// each read as a following reads it.
    pub async fn field(&self, reach: FieldReach, stop: &Stop) -> Result<FieldReading, Error> {
        let reader = LogReader::open(&self.storage_path)
            .await
            .map_err(Failure::new)?;
        let heads = reader.heads().await.map_err(Failure::new)?;
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
        for head in heads {
            match Placed::of(self, &reader, here.as_ref(), &head.run_id, None).await {
                Placed::Read(run, _) => field.rows.push(run.row(now)),
                Placed::Unreadable(why) => field.unreadable.push(why),
                Placed::Elsewhere => field.elsewhere += 1,
            }
        }
        Ok(field)
    }

    /// The repository this project works in, as far as Yunta goes: what decides
    /// whether its field is that repository's runs, and whether its checkout holds a
    /// setup, with where it stands. It reads only where the repository's git directory
    /// is, which `.git` says without a git process, and whether the config layer stands
    /// there; it reads no run. Which runs have a branch there, and so whether the
    /// repository used Yunta before, is the field's.
    pub async fn repository(&self, stop: &Stop) -> Result<Repository, Error> {
        if git_common_dir(self, stop).await?.is_none() {
            return Ok(Repository::Absent);
        }
        let dir = &self.workspace.dir;
        let layer = crate::project::repository_layer(dir);
        match tokio::fs::try_exists(&layer).await {
            Ok(true) => Ok(Repository::SetUp { dir: dir.clone() }),
            Ok(false) => Ok(Repository::NeverSetUp { dir: dir.clone() }),
            Err(unread) => Err(Failure::new(unread).into()),
        }
    }
}

/// The repository a project works in, as far as Yunta goes. Not the repository's own
/// state, which nothing here reads beyond its being one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repository {
    /// The project works in no git repository.
    Absent,
    /// A git repository whose checkout holds no config layer of Yunta's, at `dir`: never
    /// set up as far as the checkout says, whatever runs of it the field holds. `dir` is
    /// the directory a setup writes it under, which a screen names the repository by once
    /// it is set up.
    NeverSetUp { dir: PathBuf },
    /// A git repository Yunta was set up in, at `dir`: the directory the project works
    /// in, which its config layer stands under and a screen of its field names it by.
    SetUp { dir: PathBuf },
}

impl Repository {
    /// Where the repository stands, where the project works in one.
    pub fn dir(&self) -> Option<&Path> {
        match self {
            Repository::Absent => None,
            Repository::NeverSetUp { dir } | Repository::SetUp { dir } => Some(dir),
        }
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
pub(crate) struct Here {
    git_common_dir: PathBuf,
    branches: BTreeSet<RunId>,
}

impl Here {
    /// The repository `project` is rooted in, with the runs that have a branch
    /// there, or `None` outside one — where every run is listed, since there is
    /// no project to narrow to.
    pub(crate) async fn of(project: &Project, stop: &Stop) -> Result<Option<Self>, Error> {
        let Some(git_common_dir) = git_common_dir(project, stop).await? else {
            return Ok(None);
        };
        let workspace = &project.workspace;
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
    pub(crate) fn holds(&self, run_id: &RunId, project: Option<&Path>) -> bool {
        match project {
            Some(project) => project == self.git_common_dir,
            None => self.branches.contains(run_id),
        }
    }
}

/// The git directory the repository `project` works in shares with its worktrees, or
/// none outside a repository. Only git's own non-zero exit says so: a stopped or
/// unstartable git is not an answer.
async fn git_common_dir(project: &Project, stop: &Stop) -> Result<Option<PathBuf>, Error> {
    let workspace = &project.workspace;
    match yunta_engine::git::common_dir(&workspace.dir, workspace.supervision(stop)).await {
        Ok(dir) => Ok(Some(dir)),
        Err(error) if matches!(error.cause, Cause::Exit { .. }) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Where a run read for a field goes. Not [`FieldGroup`], where a run read
/// is listed.
pub(crate) enum Placed {
    /// Into the field, read, beside the registry its engine keeps.
    Read(Box<FollowedRun>, RunRegistry),
    /// Into the field, not read, for why.
    Unreadable(UnreadableRun),
    /// To another repository.
    Elsewhere,
}

impl Placed {
    /// `run` read for a field and placed by `here`: its directory, found
    /// where `status` finds it, so a run created under a since-changed
    /// `paths.runs` still lists; then `held`, the copy a following already
    /// keeps of it with its registry, so the two stand at one head; else the
    /// manifest it froze and, only when that places it here, its log through
    /// `reader`. A run with no directory or manifest to read is placed by its
    /// branch.
    pub(crate) async fn of(
        project: &Project,
        reader: &LogReader,
        here: Option<&Here>,
        run: &RunId,
        held: Option<(&FollowedRun, &RunRegistry)>,
    ) -> Placed {
        let holds = |manifest: Option<&Manifest>| {
            let project = manifest.and_then(|manifest| manifest.project.as_ref());
            let project = project.map(|project| project.git_common_dir.as_path());
            here.is_none_or(|here| here.holds(run, project))
        };
        let unread = |problem: RunUnread| {
            if holds(None) {
                Placed::Unreadable(UnreadableRun::new(run, problem))
            } else {
                Placed::Elsewhere
            }
        };
        let Some(dir) = project.run_dir(run.as_str()) else {
            let runs_root = project.runs_root.clone();
            return unread(RunUnread::NoDirectory { runs_root });
        };
        if let Some((read, registry)) = held {
            if !holds(Some(read.manifest())) {
                return Placed::Elsewhere;
            }
            return Placed::Read(Box::new(read.clone()), registry.clone());
        }
        let manifest = match frozen(&dir) {
            Ok(manifest) => manifest,
            Err(problem) => return unread(problem),
        };
        if !holds(Some(&manifest)) {
            return Placed::Elsewhere;
        }
        match logged(reader, run, dir, manifest).await {
            Ok((read, registry, _)) => Placed::Read(Box::new(read), registry),
            Err(problem) => Placed::Unreadable(UnreadableRun::new(run, problem)),
        }
    }
}

/// `run` read whole through `reader`, as a following reads the run it follows: its
/// directory, found where `status` finds it, the manifest it froze and its log, folded into
/// a [`FollowedRun`], with the registry its engine keeps beside it and every event of its
/// log; or unreadable, with why, as a field says a run it cannot read.
pub(crate) async fn read_whole(
    project: &Project,
    reader: &LogReader,
    run: &RunId,
) -> Result<(FollowedRun, RunRegistry, Vec<StoredEvent>), UnreadableRun> {
    let unread = |problem| UnreadableRun::new(run, problem);
    let Some(dir) = project.run_dir(run.as_str()) else {
        let runs_root = project.runs_root.clone();
        return Err(unread(RunUnread::NoDirectory { runs_root }));
    };
    let manifest = frozen(&dir).map_err(unread)?;
    logged(reader, run, dir, manifest).await.map_err(unread)
}

/// The manifest the run whose directory is `dir` froze, or why it does not read.
fn frozen(dir: &Path) -> Result<Manifest, RunUnread> {
    let path = yunta_engine::run_dir::manifest_path(dir);
    match read_manifest(&path) {
        Ok(manifest) => Ok(manifest.doc),
        Err(_) => Err(RunUnread::Manifest { path }),
    }
}

/// `run`, whose directory is `dir` and which froze `manifest`, as every event of its log
/// read through `reader` derives it, with the registry its engine keeps beside it and those
/// events; or why its log does not read.
async fn logged(
    reader: &LogReader,
    run: &RunId,
    dir: PathBuf,
    manifest: Manifest,
) -> Result<(FollowedRun, RunRegistry, Vec<StoredEvent>), RunUnread> {
    let events = reader
        .events(run)
        .await
        .map_err(|unread| RunUnread::EventLog(unread.to_string()))?;
    let registry = RunRegistry::at(dir);
    let read = FollowedRun::read(run.clone(), manifest, &events, registry.engine());
    Ok((read, registry, events))
}
