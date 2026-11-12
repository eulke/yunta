//! See [`crate`]. A checkout of its own for a run that must not touch the
//! project: a temporary root, and under it a worktree seeded with the
//! repository's `.yunta`, so a run inside it resolves its workflow through
//! the very catalog a real run resolves through. Whoever builds one writes
//! what the run needs on top, then commits the seed; the project the run is
//! made in keeps its runs, checkouts and log under the root, so the
//! project's own are never written to. What `doctor --session` probes a
//! session in, and what each of a project's cases runs in.

use std::path::{Path, PathBuf};

use yunta_engine::process::Supervision;

use crate::{Error, Failure, Project};

/// A checkout of its own for a run that must not touch the project.
pub(crate) struct Sandbox {
    /// Owns the temporary root, so the sandbox lasts exactly as long as
    /// this value and not a moment longer.
    root: tempfile::TempDir,
    worktree: PathBuf,
}

impl Sandbox {
    /// A sandbox seeded from `repo`'s `.yunta`.
    pub(crate) fn seeded(repo: &Path) -> Result<Self, Error> {
        let root = tempfile::tempdir().map_err(|e| unmade("create a sandbox".to_string(), e))?;
        let worktree = root.path().join("worktree");
        std::fs::create_dir_all(&worktree)
            .map_err(|e| unmade(format!("create {}", worktree.display()), e))?;
        let catalog = repo.join(".yunta");
        if catalog.is_dir() {
            // Created here rather than left to the first entry that happens
            // to be a directory: a catalog holding only files would
            // otherwise be copied into a directory that is not there.
            let into = worktree.join(".yunta");
            std::fs::create_dir_all(&into)
                .map_err(|e| unmade(format!("create {}", into.display()), e))?;
            copy_dir_all(&catalog, &into)
                .map_err(|e| unmade(format!("copy the catalog from {}", catalog.display()), e))?;
        }
        Ok(Sandbox { root, worktree })
    }

    /// The tree the run works in.
    pub(crate) fn worktree(&self) -> &Path {
        &self.worktree
    }

    /// Copies `seed`'s tree on top of the worktree, before it is committed.
    pub(crate) fn seed(&self, seed: &Path) -> Result<(), Error> {
        copy_dir_all(seed, &self.worktree)
            .map_err(|e| unmade(format!("seed the sandbox from {}", seed.display()), e))
    }

    /// Makes the worktree a repository whose initial commit holds the seed
    /// and what was written on it, or nothing, so a run's scope diff only
    /// ever shows what its sessions changed.
    pub(crate) async fn commit_seed(&self, supervision: Supervision<'_>) -> Result<(), Error> {
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "yunta-test@localhost"],
            vec!["config", "user.name", "yunta-test"],
            vec!["add", "-A"],
            vec!["commit", "-q", "--allow-empty", "-m", "sandbox"],
        ] {
            let effect = yunta_engine::git::Effect::Checkout;
            yunta_engine::git::change(&self.worktree, effect, &args, supervision).await?;
        }
        Ok(())
    }
}

impl Project {
    /// This project made in `sandbox`: its config, with its runs, its
    /// checkouts and its log under the sandbox's root, working in its tree.
    pub(crate) fn sandboxed(&self, sandbox: &Sandbox) -> Project {
        let root = sandbox.root.path();
        Project {
            config: self.config.clone(),
            runs_root: root.join("runs"),
            worktrees_root: root.join("worktrees"),
            storage_path: root.join("events.db"),
            user_root: self.user_root.clone(),
            workspace: crate::Workspace {
                dir: sandbox.worktree.clone(),
                ..self.workspace.clone()
            },
        }
    }
}

/// Copies `from`'s tree into `into`, which already exists. Every entry is
/// copied as a regular file or directory; the seed is repository content a
/// run commits, never a place for symlinks.
fn copy_dir_all(from: &Path, into: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = into.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// A step of making a sandbox the disk refused: what it was doing.
fn unmade(doing: String, source: std::io::Error) -> Error {
    Failure::new(SandboxUnmade { doing, source }).into()
}

/// A sandbox the disk would not make: what it was doing, and its words.
#[derive(Debug, thiserror::Error)]
#[error("failed to {doing}: {source}")]
struct SandboxUnmade {
    doing: String,
    source: std::io::Error,
}
