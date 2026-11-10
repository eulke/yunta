//! A bench's world read as a project: the bench's runs root and its log, under a
//! clock that stands still, so what the API reads of a run is what the bench wrote.

use std::path::PathBuf;
use std::sync::Arc;

use yunta_api::{Project, Workspace};
use yunta_core::{ConfigLayer, Env, ProcessSecrets, SystemIdSource};
use yunta_testkit::Bench;
use yunta_testkit_core::FixedClock;

/// The project whose runs are `bench`'s.
pub fn project_of(bench: &Bench) -> Project {
    Project {
        config: ConfigLayer::default(),
        runs_root: bench.runs_root.clone(),
        worktrees_root: bench.runs_root.with_file_name("worktrees"),
        storage_path: bench.store_path().to_path_buf(),
        user_root: bench.runs_root.with_file_name("user"),
        workspace: Workspace {
            dir: bench.worktree.clone(),
            env: Env::default(),
            clock: Arc::new(FixedClock),
            ids: Arc::new(SystemIdSource),
            secrets: Arc::new(ProcessSecrets),
            binary: PathBuf::from("yunta"),
        },
    }
}
