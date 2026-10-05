//! A repository that ignores `target/`, and the pool of a run over it:
//! the world every test of the project's checkouts starts from.
//!
//! Every test binary that declares `mod pool_world` compiles the whole
//! module and uses a part of it, so what another binary needs reads as
//! dead here.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_core::{CommitSha, RunId};
use yunta_engine::{CheckoutPool, Unit, UnitHome, UnitId};
use yunta_testkit::{git, git_output, init_repo, Owner};

pub const RUN: &str = "01JPOOLEDCHECKOUTS0000000A";

/// A repository that ignores `target/`, and the pool of a run over it.
pub struct World {
    pub root: tempfile::TempDir,
    pub repo: PathBuf,
    pub run: RunId,
    pub pool: Arc<CheckoutPool>,
    pub owner: Owner,
}

impl World {
    pub fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        ignore_builds(&repo);
        let pool = World::pool_of(root.path(), &repo, "run");
        World {
            root,
            repo,
            run: RunId::from(RUN),
            pool,
            owner: Owner::new(),
        }
    }

    /// The project's pool, as the run whose directory is `run_dir` under
    /// `root` reaches it.
    pub fn pool_of(root: &Path, repo: &Path, run_dir: &str) -> Arc<CheckoutPool> {
        CheckoutPool::new(&root.join("worktrees"), repo, &root.join(run_dir))
    }

    pub fn head(&self) -> CommitSha {
        git_output(&self.repo, &["rev-parse", "HEAD"])
            .trim()
            .parse()
            .unwrap()
    }

    /// Opens `task`'s first attempt, handing back the hold on its checkout.
    pub async fn open(&self, task: &str) -> (Unit, yunta_engine::Lease) {
        self.open_through(&self.pool, &self.run, task, 1).await
    }

    pub async fn open_through(
        &self,
        pool: &Arc<CheckoutPool>,
        run: &RunId,
        task: &str,
        attempt: u32,
    ) -> (Unit, yunta_engine::Lease) {
        pool.open(
            UnitHome {
                repo: &self.repo,
                run_dir: &self.root.path().join("run"),
                run_id: run,
                base: &self.head(),
            },
            UnitId::Task(task.into()),
            attempt,
            self.owner.supervision(),
        )
        .await
        .expect("a unit opens in the pool")
    }

    /// What landing does to a unit's checkout: it lets go of the branch.
    pub async fn land(&self, unit: &Unit) {
        yunta_engine::retire_unit_branch(unit, &self.repo, self.owner.supervision())
            .await
            .unwrap();
    }

    /// The checkouts the pool holds on disk, by name.
    pub async fn slots(&self) -> Vec<String> {
        let home = self.pool.home(self.owner.supervision()).await.unwrap();
        checkouts_in(&home)
    }
}

/// Commits a `.gitignore` that ignores `target/` in `repo`.
pub fn ignore_builds(repo: &Path) {
    std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "ignore builds"]);
}

/// The checkouts under a pool's directory, by name.
pub fn checkouts_in(home: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(home)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

pub fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}

pub fn delete(path: &Path) {
    std::fs::remove_file(path).unwrap();
}

pub fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}
