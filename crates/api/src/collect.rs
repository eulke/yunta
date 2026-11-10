//! See [`crate`]. Collecting what a project's runs leave: terminal runs once
//! `storage.retention_days` has passed since their last event, and the
//! project's pool tidied.
//!
//! A finished run's `run.dir` can be deleted without losing anything:
//! `events.jsonl` already exported a self-contained copy at close. Its
//! checkout is the project's pool's, so it goes back to the pool with its
//! build rather than being removed; only a checkout a run made before its
//! project kept a pool is taken away. A parked run nobody came back to
//! within retention gives its checkout back the same way, unless a session
//! it left waits to be picked up there.
//!
//! A run is found and reclaimed by the paths *frozen in its manifest*, not
//! the current config's: `run.dir` through the same search rule a run is
//! opened by, and the worktree through the root the manifest froze, so a
//! `paths.*` change after the run was created never orphans either. A
//! removal that fails is said and left uncounted; only a run whose whole
//! footprint was actually reclaimed is reported as such.
//!
//! Database retention: the event-log rows have their own deadline, with one
//! explicit death order: run.dir (whose exported `events.jsonl` is the
//! self-contained copy) dies first, rows die on a *later* pass, only for a
//! run whose run.dir is already gone. The database is never the first copy
//! of a run to die.
//!
//! What the pass did is render's [`Collected`], filled as it goes; each
//! thing it could not do is said through the caller's remarks as it
//! happens, and counted there. What it does to the runs past retention is
//! `runs`'s, and what it does to the pool `pool`'s.

mod pool;
mod runs;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_core::RunId;
use yunta_engine::process::Supervision;
use yunta_engine::CheckoutPool;
use yunta_render::page::{Collected, Reclaim, Uncollected};

use crate::{Error, Project, Remark, Remarks, Stop};

/// How far a pass of collection goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collect {
    /// Everything the pass finds, done.
    Pass,
    /// What a pass would do, said, with nothing touched.
    DryRun,
}

impl Project {
    /// Collects what this project's runs left: every terminal run past
    /// retention, its files on this pass and its rows on a later one, the
    /// checkouts runs nobody drives gave back or kept before the pool, and
    /// the pool tidied within its budget. Answers what the pass did, or
    /// under [`Collect::DryRun`] would; each thing it could not do is said
    /// through `remarks` as it happens.
    pub async fn collect(
        &self,
        collect: Collect,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<Collected, Error> {
        let limits = self
            .config
            .storage
            .as_ref()
            .and_then(|storage| storage.pool.as_ref());
        let pool = CheckoutPool::upkeep(
            &self.worktrees_root,
            &self.workspace.dir,
            limits,
            Arc::clone(&self.workspace.clock),
        );
        let mut collection = Collection {
            project: self,
            pool,
            remarks,
            stop,
            collected: Collected::new(collect == Collect::DryRun),
        };
        collection.pass().await?;
        Ok(collection.collected)
    }
}

/// One pass of collection: the project it collects, the pool it tidies,
/// where it says what it could not do, and what it did so far.
struct Collection<'a> {
    project: &'a Project,
    pool: Arc<CheckoutPool>,
    remarks: &'a dyn Remarks,
    stop: &'a Stop,
    collected: Collected,
}

/// What a pass collected: the runs whose files went, and the checkouts of
/// the project's pool runs gave back: a collected run's, and a parked run's
/// nobody came back to.
#[derive(Default)]
struct Reclaimed {
    removed: Vec<RunId>,
    given_back: Vec<(RunId, PathBuf)>,
    /// Where each run nothing drives kept checkouts before the project's
    /// pool: its units' directory, and its own tree when outside the pool.
    idle: Vec<(RunId, Vec<PathBuf>)>,
}

impl<'a> Collection<'a> {
    /// The whole pass: the runs past retention reclaimed, what git still
    /// holds of the collected ones forgotten, then the pool's part: the
    /// checkouts given back, the units' branches kept by refs, the old
    /// layouts taken away and the builds tidied; or, under a dry run, what
    /// the pool would keep.
    async fn pass(&mut self) -> Result<(), Error> {
        let Reclaimed {
            removed,
            given_back,
            idle,
        } = self.reclaim().await?;
        for run_id in &removed {
            self.forget_units(run_id).await;
        }
        if self.collected.dry_run {
            self.would_adopt().await;
            match self.pool.report(self.supervision()).await {
                Ok(kept) => self.collected.now = Some(kept.to_string()),
                Err(e) => self.problem(Uncollected::Checkouts {
                    failed: e.to_string(),
                }),
            }
            return Ok(());
        }
        for (run_id, tree) in &given_back {
            self.give_back(run_id, tree).await;
        }
        self.adopt().await;
        for (run_id, places) in &idle {
            self.take_old_layouts(run_id, places).await;
        }
        self.tidy().await;
        Ok(())
    }

    /// What a process the pass starts answers to: the caller's stop, the
    /// workspace's clock, environment and path.
    fn supervision(&self) -> Supervision<'a> {
        self.project.workspace.supervision(self.stop)
    }

    /// Where this workspace works.
    fn here(&self) -> &'a Path {
        &self.project.workspace.dir
    }

    /// `dir` as a person reads it: inside this workspace or the home
    /// directory, written from there.
    fn shown(&self, dir: &Path) -> String {
        let home = self.project.workspace.env.home.as_deref();
        surco::paths::shown(dir, self.here(), home)
    }

    /// Something the pass could not do: said as it happens, and counted on
    /// what the pass answers.
    fn problem(&mut self, uncollected: Uncollected) {
        self.remarks.remark(Remark::Uncollected(uncollected));
        self.collected.problems += 1;
    }

    /// Something of run `run` the pass could not do, `failed` in its own
    /// words.
    fn run_problem(&mut self, run: &RunId, failed: impl std::fmt::Display) {
        self.problem(Uncollected::Run {
            run: run.clone(),
            failed: failed.to_string(),
        });
    }

    /// Gives a run's checkout back to the project's pool, with no branch.
    async fn give_back(&mut self, run_id: &RunId, tree: &Path) {
        let released =
            yunta_engine::release_run_checkout(&self.pool, tree, run_id, self.supervision());
        if let Err(e) = released.await {
            self.run_problem(run_id, e);
        }
    }

    /// What git still holds of a run whose files are gone: the checkouts its
    /// units worked in, and the branches they worked on, a blocked task's
    /// left work among them, which nothing reads once the run is collected.
    /// The project's checkouts stay, with no branch, for the runs to come.
    async fn forget_units(&mut self, run_id: &RunId) {
        let supervision = self.supervision();
        if let Err(e) = yunta_engine::prune_worktrees(self.here(), supervision).await {
            self.run_problem(run_id, e);
            return;
        }
        let forgotten =
            yunta_engine::forget_run_units(&self.pool, self.here(), run_id, supervision).await;
        if let Err(e) = forgotten {
            self.run_problem(run_id, e);
        }
    }

    /// Takes away the clean checkouts run `run_id`, which nothing drives,
    /// kept at `places` before the project's pool, and names the ones kept.
    async fn take_old_layouts(&mut self, run_id: &RunId, places: &[PathBuf]) {
        for place in places {
            let taken =
                yunta_engine::remove_old_checkouts(self.here(), place, self.supervision()).await;
            let done = match taken {
                Ok(done) => done,
                Err(e) => {
                    self.run_problem(run_id, e);
                    continue;
                }
            };
            for checkout in done {
                let run = run_id.clone();
                self.collected.did(match checkout {
                    yunta_engine::OldCheckout::Removed(path) => Reclaim::OldCheckoutRemoved {
                        checkout: self.shown(&path),
                        run,
                    },
                    yunta_engine::OldCheckout::Kept(path) => Reclaim::OldCheckoutKept {
                        checkout: self.shown(&path),
                        run,
                    },
                });
            }
        }
    }
}
