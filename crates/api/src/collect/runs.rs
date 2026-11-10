//! See [`super`]. What a pass of collection does to the runs past retention:
//! a terminal run's files removed on this pass and its rows on a later one, a
//! parked run's checkout given back, and the checkouts a run nothing drives
//! kept before the project's pool noted for taking away.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use yunta_core::events::{OrphanedSession, StoredEvent};
use yunta_core::{Isolation, RunId};
use yunta_render::page::{Reclaim, Uncollected};
use yunta_replay::registry::Engine;
use yunta_replay::RunState;
use yunta_storage::Storage;

use super::{Collection, Reclaimed};
use crate::{Error, Failure};

impl Collection<'_> {
    /// Removes every terminal run past retention, its files on this pass,
    /// its rows on a later one, and answers the runs whose files went, with
    /// the checkouts of the project's pool to give back: a collected run's,
    /// and that of a parked run nobody came back to within retention.
    pub(super) async fn reclaim(&mut self) -> Result<Reclaimed, Error> {
        let retention_days = self
            .project
            .config
            .storage
            .as_ref()
            .and_then(|storage| storage.retention_days);
        let Some(retention_days) = retention_days else {
            self.collected.unretained = true;
            return Ok(Reclaimed::default());
        };
        let storage = Storage::open(&self.project.storage_path).map_err(Failure::new)?;
        let runs = storage.list_runs().map_err(Failure::new)?;
        let past = Retention {
            now: self.project.workspace.clock.now(),
            days: i64::from(retention_days),
        };
        let mut reclaimed = Reclaimed::default();
        for run in runs {
            let events = match storage.events_for_run(&run.run_id) {
                Ok(events) => events,
                Err(e) => {
                    self.run_problem(&run.run_id, e);
                    continue;
                }
            };
            self.reclaim_run(&storage, run.run_id, &events, past, &mut reclaimed)
                .await;
        }
        Ok(reclaimed)
    }

    /// What the pass does to run `run_id`, whose log is `events`: a run
    /// nothing drives has its old layouts noted; one past `past` gives its
    /// parked checkout back, or, ended, loses its files, or its rows once
    /// its files are gone.
    async fn reclaim_run(
        &mut self,
        storage: &Storage,
        run_id: RunId,
        events: &[StoredEvent],
        past: Retention,
        reclaimed: &mut Reclaimed,
    ) {
        let Some(last) = events.last() else { return };
        let state = RunState::replay(events);
        let ended = state.ended();
        if !ended {
            if let Some(places) = self.old_layouts(&run_id, &state).await {
                reclaimed.idle.push((run_id.clone(), places));
            }
        }
        if !past.passed(last.timestamp) {
            return;
        }
        if !ended {
            if let Some(tree) = self.parked_checkout(&state).await {
                reclaimed.given_back.push((run_id, tree));
            }
            return;
        }
        // Death order: files first, rows on a later pass. A run whose
        // run.dir is still on disk (found by its frozen paths) loses only
        // the files this pass; one whose run.dir is already gone (a
        // previous pass, or a person) has its rows purged now.
        let Some(run_dir) = self.project.run_dir(run_id.as_str()) else {
            if self.purge_rows(storage, &run_id, events.len()) {
                self.collected.reclaimed += 1;
            }
            return;
        };
        let bound = state.run.checkout();
        if let Some(tree) = self.worktree_of(&run_dir, &run_id, bound) {
            if self.pool_owns(&tree).await == Some(true) {
                reclaimed.given_back.push((run_id.clone(), tree));
            }
        }
        if self.remove_run(&run_dir, &run_id, bound).await {
            self.collected.reclaimed += 1;
            if !self.collected.dry_run {
                reclaimed.removed.push(run_id);
            }
        }
    }

    /// Purges the rows of a terminal run whose `run.dir` is already gone,
    /// and answers whether they went; under a dry run they only would.
    fn purge_rows(&mut self, storage: &Storage, run: &RunId, events: usize) -> bool {
        if self.collected.dry_run {
            let run = run.clone();
            self.collected.did(Reclaim::Purged { run, events });
            return true;
        }
        match storage.purge_run(run) {
            Ok(purged) => {
                let run = run.clone();
                let events = purged.rows;
                self.collected.did(Reclaim::Purged { run, events });
                true
            }
            Err(e) => {
                self.run_problem(run, e);
                false
            }
        }
    }

    /// Where a run nothing drives kept checkouts before the project's pool:
    /// its units' directory under its own, and its own tree when the pool
    /// does not keep it. `None` for a run an engine drives now, or may.
    async fn old_layouts(&mut self, run_id: &RunId, state: &RunState) -> Option<Vec<PathBuf>> {
        let run_dir = self.project.run_dir(run_id.as_str())?;
        let engine = yunta_engine::read_engine(&run_dir, &yunta_engine::lock::SystemProbe);
        if !matches!(engine, Engine::Dead | Engine::Unrecorded) {
            return None;
        }
        let mut places = vec![yunta_engine::run_dir::unit_worktrees(&run_dir)];
        let bound = state.run.checkout();
        if let Some(tree) = self.worktree_of(&run_dir, run_id, bound) {
            if self.pool_owns(&tree).await == Some(false) {
                places.push(tree);
            }
        }
        Some(places)
    }

    /// Whether `tree` is a checkout of the project's pool, which a run gives
    /// back rather than takes away. `None` when the pool cannot say: the
    /// checkout is then left where it is, and the reason is said.
    async fn pool_owns(&mut self, tree: &Path) -> Option<bool> {
        match self.pool.owns(tree, self.supervision()).await {
            Ok(owned) => Some(owned),
            Err(e) => {
                let checkout = self.shown(tree);
                let failed = e.to_string();
                self.problem(Uncollected::LeftAlone { checkout, failed });
                None
            }
        }
    }

    /// The checkout of the project's pool a parked run works in, when nobody
    /// would lose anything if it went back: no session the run left open is
    /// waiting to be picked up there. Its work stays on its branch, and a
    /// wake takes a checkout again.
    async fn parked_checkout(&mut self, state: &RunState) -> Option<PathBuf> {
        let tree = state.run.checkout()?;
        let continued = state
            .nodes
            .iter()
            .any(|(_, record)| matches!(record.orphaned_session, Some(OrphanedSession::Open(_))));
        if continued {
            return None;
        }
        (self.pool_owns(tree).await == Some(true)).then(|| tree.to_path_buf())
    }

    /// Removes a terminal run's on-disk footprint, its `run.dir` and, for a
    /// worktree-isolated run, the worktree frozen in its manifest, and
    /// answers whether every directory that was present got removed. A run
    /// with a removal that failed is said and answers `false`, so it is
    /// never counted as reclaimed while some of its disk survives. Under a
    /// dry run nothing is removed and every present directory counts as if
    /// it had been.
    async fn remove_run(&mut self, run_dir: &Path, run_id: &RunId, bound: Option<&Path>) -> bool {
        let tree = self
            .worktree_of(run_dir, run_id, bound)
            .filter(|tree| tree.exists());
        // The worktree goes through git, which unregisters it as well. A
        // checkout of the project's pool is given back, never taken away.
        let tree_gone = match tree {
            Some(tree) => self.remove_tree(&tree).await,
            None => Removal::Absent,
        };
        let dir_gone = self.remove_dir(run_dir).await;
        let removed_any = tree_gone == Removal::Done || dir_gone == Removal::Done;
        removed_any && tree_gone != Removal::Failed && dir_gone != Removal::Failed
    }

    /// Removes a run's own checkout, unless the project's pool keeps it.
    async fn remove_tree(&mut self, tree: &Path) -> Removal {
        match self.pool_owns(tree).await {
            Some(true) => Removal::Absent,
            None => Removal::Failed,
            Some(false) if self.collected.dry_run => {
                self.collected.did(Reclaim::Removed {
                    at: self.shown(tree),
                });
                Removal::Done
            }
            Some(false) => match yunta_engine::remove_checkout(tree, self.supervision()).await {
                Ok(()) => {
                    self.collected.did(Reclaim::Removed {
                        at: self.shown(tree),
                    });
                    Removal::Done
                }
                Err(e) => {
                    let at = self.shown(tree);
                    let failed = e.to_string();
                    self.problem(Uncollected::Unremoved { at, failed });
                    Removal::Failed
                }
            },
        }
    }

    /// Removes a run's directory, when it is there.
    async fn remove_dir(&mut self, run_dir: &Path) -> Removal {
        if !run_dir.exists() {
            return Removal::Absent;
        }
        let at = self.shown(run_dir);
        if self.collected.dry_run {
            self.collected.did(Reclaim::Removed { at });
            return Removal::Done;
        }
        match tokio::fs::remove_dir_all(run_dir).await {
            Ok(()) => {
                self.collected.did(Reclaim::Removed { at });
                Removal::Done
            }
            Err(e) => {
                let failed = e.to_string();
                self.problem(Uncollected::Unremoved { at, failed });
                Removal::Failed
            }
        }
    }

    /// The worktree directory the pass reclaims for this run, read from the
    /// root frozen in its manifest, or `None` when there is none to reclaim:
    /// an `isolation: none` run works on the checkout itself, never a
    /// dedicated worktree, so nothing is removed for it beyond `run.dir`. A
    /// manifest that cannot be read is said and treated as no worktree:
    /// `run.dir` is still reclaimed, its worktree (if any) left for a person,
    /// never guessed at from the current config.
    fn worktree_of(
        &mut self,
        run_dir: &Path,
        run_id: &RunId,
        bound: Option<&Path>,
    ) -> Option<PathBuf> {
        let path = yunta_engine::run_dir::manifest_path(run_dir);
        let manifest = match crate::run::read_manifest(&path) {
            Ok(manifest) => manifest.doc,
            Err(failure) => {
                self.run_problem(run_id, failure.said());
                return None;
            }
        };
        match manifest.isolation {
            Isolation::Worktree => Some(bound.map(Path::to_path_buf).unwrap_or_else(|| {
                self.project
                    .worktrees_root_for(&manifest)
                    .join(run_id.as_str())
            })),
            Isolation::None => None,
        }
    }
}

/// How one directory of a run's footprint fared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Removal {
    /// It went, or under a dry run would.
    Done,
    /// It was not there to remove, or the project's pool keeps it.
    Absent,
    /// It could not go, and the reason was said.
    Failed,
}

/// The age past which a run is collected: retention's days, read at the
/// pass's instant.
#[derive(Debug, Clone, Copy)]
struct Retention {
    now: DateTime<Utc>,
    days: i64,
}

impl Retention {
    /// Whether a run whose last event is at `last` is past retention.
    fn passed(self, last: DateTime<Utc>) -> bool {
        (self.now - last).num_days() >= self.days
    }
}
