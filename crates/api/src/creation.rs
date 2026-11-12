//! See [`crate`]. A run created: the admitted run born on the log, its id
//! minted from the workspace's clock and id source, and the tree it works in
//! prepared — a checkout of the project's pool on a branch of its own, or
//! this checkout for a run that isolates nothing. Driving it here and handing
//! it to a detached process both start from the run this makes, so both reach
//! a run the same way.

use std::path::{Path, PathBuf};

use yunta_core::{Isolation, Manifest, ModeName, Redactor, RunId};
use yunta_engine::{CreateRunParams, FrozenRun};
use yunta_render::document::PreRunWarnings;
use yunta_render::refusal::Refused;
use yunta_replay::history::PriorEstimation;
use yunta_storage::AsyncStorage;

use crate::{Admission, Error, Failure, Project, Remark, Remarks, Stop};

/// A run born on the log, its tree prepared, not yet driven.
///
/// Not the run as a page draws it: what a caller drives or hands on.
pub struct CreatedRun<'p> {
    pub(crate) project: &'p Project,
    pub id: RunId,
    /// The directory the run keeps its manifest, its lock and its documents
    /// in.
    pub dir: PathBuf,
    /// The tree the run works in.
    pub worktree: PathBuf,
    pub manifest: Manifest,
    /// What this workflow's past runs cost, for the comparison a run's
    /// closing draws.
    pub prior: Option<PriorEstimation>,
    /// Every caution said before the first token.
    pub warnings: PreRunWarnings,
}

impl<'p> Admission<'p> {
    /// The admitted run, created: refused at no cost when the project already
    /// has as many runs going as `limits.max_concurrent_runs` allows, or
    /// when a secret it declares is too short to redact. A dead process's
    /// lock on this checkout, which a run that isolates nothing takes over,
    /// is said through `remarks`.
    pub async fn create(self, remarks: &dyn Remarks, stop: &Stop) -> Result<CreatedRun<'p>, Error> {
        let project = self.project;
        let storage = project.storage().await?;
        let manifest = &self.frozen.manifest;
        refuse_past_the_cap(&storage, manifest).await?;
        // Resolved before anything is made: a short declared secret refuses
        // the run here, before its identifier is minted, a checkout is taken
        // or a branch is made.
        let secrets = project.workspace.secrets.as_ref();
        let redactor = yunta_engine::run_redactor(manifest, secrets)?;
        let workspace = &project.workspace;
        let id = workspace.ids.mint_run_id(workspace.clock.now());
        let worktree = project.tree_for(manifest, &id, remarks, stop).await?;
        let birth = Birth {
            frozen: &self.frozen,
            id: &id,
            mode: &self.mode,
            worktree: &worktree,
            redactor: &redactor,
        };
        let dir = project.born(birth, &storage, stop).await?;
        Ok(CreatedRun {
            project,
            id,
            dir,
            worktree,
            manifest: self.frozen.manifest,
            prior: self.prior,
            warnings: self.warnings,
        })
    }
}

/// What a run is born with: the run it froze, its id and mode, the tree it
/// works in, and what its log is redacted with.
struct Birth<'a> {
    frozen: &'a FrozenRun,
    id: &'a RunId,
    mode: &'a ModeName,
    worktree: &'a Path,
    redactor: &'a Redactor,
}

impl Project {
    /// The tree run `id` of `manifest` works in, prepared: a checkout of the
    /// project's pool on a branch of its own — warm with whatever the last
    /// run there built, or, for a run whose units build, the one holding the
    /// least — or, for a run that isolates nothing, this checkout.
    async fn tree_for(
        &self,
        manifest: &Manifest,
        id: &RunId,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<PathBuf, Error> {
        let workspace = &self.workspace;
        let supervision = workspace.supervision(stop);
        match manifest.isolation {
            Isolation::Worktree => {
                let pool = self.pool_of(manifest, &self.runs_root.join(id.as_str()));
                let birth = yunta_engine::Birth::of(&manifest.workflow);
                let base = &manifest.base_commit;
                let (checkout, _held) = pool.open_run(id, base, birth, supervision).await?;
                Ok(checkout)
            }
            Isolation::None => {
                let worktree = self.run_tree(manifest, None, id, &workspace.dir);
                let prepared = yunta_engine::prepare_worktree(
                    &workspace.dir,
                    &worktree,
                    &manifest.base_commit,
                    &id.branch(),
                    manifest.isolation,
                    supervision,
                )
                .await?;
                if let yunta_engine::WorktreePrepared::StoleStaleLock { dead_pid } = prepared {
                    remarks.remark(Remark::StaleLockTaken { pid: dead_pid });
                }
                Ok(worktree)
            }
        }
    }

    /// The run `birth` describes, born on the log under this project's runs
    /// root: the root of its lineage, holding every document its `inputs:`
    /// named, in the environment its sessions inherit.
    async fn born(
        &self,
        birth: Birth<'_>,
        storage: &AsyncStorage,
        stop: &Stop,
    ) -> Result<PathBuf, Error> {
        let env = &self.workspace.env;
        let dir = yunta_engine::create_run(
            CreateRunParams {
                run_id: birth.id,
                manifest: &birth.frozen.manifest,
                runs_root: &self.runs_root,
                mode: birth.mode,
                worktree: birth.worktree,
                promoted_from: None,
                // The run is born holding every document its `inputs:`
                // named, accepted right after `run_created`.
                artifacts: &birth.frozen.documents,
                blobs: &birth.frozen.blobs,
                // A run a caller starts is the root of its lineage: it
                // measures on its first wake, if its config names a suite.
                baseline: None,
                environment: yunta_engine::process::execution_environment(Some(env)),
                redactor: birth.redactor,
            },
            storage,
            self.workspace.supervision(stop),
        )
        .await?;
        Ok(dir)
    }
}

/// Refuses a run past `limits.max_concurrent_runs`: a soft budget, not a
/// safety limit — two runs created at once can both pass the count — checked
/// before anything is made, so the refusal costs nothing. A paused run holds
/// a slot, since it expects a resume; a closed or broken one never does.
async fn refuse_past_the_cap(storage: &AsyncStorage, manifest: &Manifest) -> Result<(), Error> {
    let cap = manifest
        .config
        .limits
        .as_ref()
        .and_then(|limits| limits.max_concurrent_runs);
    let Some(cap) = cap else {
        return Ok(());
    };
    let mut active = 0;
    for run in storage.list_runs().await.map_err(Failure::new)? {
        let events = storage
            .events_for_run(run.run_id)
            .await
            .map_err(Failure::new)?;
        if !events.is_empty() && !yunta_replay::RunState::replay(&events).ended() {
            active += 1;
        }
    }
    match active >= cap as usize {
        true => Err(Refused::TooManyRuns { active, cap }.into()),
        false => Ok(()),
    }
}
