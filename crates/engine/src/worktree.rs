//! Working-tree isolation (T4.2, Contrato §7.3).
//!
//! `worktree` (default): each run gets its own `git worktree`, checked
//! out from the manifest's frozen `base_commit` — concurrent runs on the
//! same repo never collide, and the user's own checkout stays untouched
//! by the agent's edits. `none`: the run operates directly on the given
//! checkout — legitimate for watching an agent edit live, or CI already
//! inside an ephemeral container — but the engine **requires a clean
//! tree** at the start (I3/§6: without that, scope-by-diff can't tell
//! the agent's work from the user's) and refuses a second concurrent run
//! on that same repo via a lock file next to git's own metadata.
//!
//! Cleanup (`on_finish.cleanup: worktree`) is out of M-0 — `on_finish:`
//! doesn't exist in the schema recorte yet. A prepared worktree is left
//! on disk after the run for inspection; only `none`'s lock is released,
//! since holding it forever would make every run after the first
//! permanently refuse to start.

use std::path::{Path, PathBuf};

use thiserror::Error;
use yunta_core::Isolation;

#[derive(Debug, Error)]
pub enum WorktreeError {
    #[error("git {args} in `{cwd}` failed: {detail}")]
    Git {
        args: String,
        cwd: PathBuf,
        detail: String,
    },
    #[error(
        "`{path}` has uncommitted changes — isolation `none` requires a clean tree (§7.3): \
         without it, scope-by-diff can't tell the agent's work from what was already there"
    )]
    DirtyTree { path: PathBuf },
    #[error(
        "`{path}` already has a run in progress under isolation `none` — only one at a time \
         is allowed on the same checkout (§7.3); use isolation `worktree` to run concurrently"
    )]
    Locked { path: PathBuf },
    #[error("failed to {action} for `{path}`")]
    Io {
        action: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Puts `repo` in the state a run needs before it starts: for
/// `Isolation::Worktree`, creates a dedicated `git worktree` at
/// `worktree_path` on a new branch `branch_name`, checked out at
/// `base_commit`; for `Isolation::None`, verifies `repo` itself is clean
/// and takes its lock (`worktree_path` is ignored — the run operates on
/// `repo` directly).
pub async fn prepare_worktree(
    repo: &Path,
    worktree_path: &Path,
    base_commit: &str,
    branch_name: &str,
    isolation: Isolation,
) -> Result<(), WorktreeError> {
    match isolation {
        Isolation::Worktree => {
            if let Some(parent) = worktree_path.parent() {
                std::fs::create_dir_all(parent).map_err(|source| WorktreeError::Io {
                    action: "create the worktrees directory".to_string(),
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            run_git(
                repo,
                &[
                    "worktree",
                    "add",
                    &worktree_path.display().to_string(),
                    "-b",
                    branch_name,
                    base_commit,
                ],
            )
            .await?;
            Ok(())
        }
        Isolation::None => {
            if !is_clean(repo).await? {
                return Err(WorktreeError::DirtyTree {
                    path: repo.to_path_buf(),
                });
            }
            lock(repo).await
        }
    }
}

/// Releases what `prepare_worktree` took: for `None`, removes the lock
/// so a later run may proceed. For `Worktree`, a no-op — the worktree
/// stays on disk (see module docs).
pub async fn release_worktree(repo: &Path, isolation: Isolation) -> Result<(), WorktreeError> {
    match isolation {
        Isolation::Worktree => Ok(()),
        Isolation::None => {
            let lock_path = lock_path(repo).await?;
            match std::fs::remove_file(&lock_path) {
                Ok(()) => Ok(()),
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(source) => Err(WorktreeError::Io {
                    action: "remove the isolation lock".to_string(),
                    path: lock_path,
                    source,
                }),
            }
        }
    }
}

async fn is_clean(repo: &Path) -> Result<bool, WorktreeError> {
    let output = run_git(repo, &["status", "--porcelain"]).await?;
    Ok(output.trim().is_empty())
}

/// The lock lives next to git's own metadata (`--git-common-dir`, correct
/// even when `repo` is itself already a worktree) rather than inside the
/// working tree — it must never show up as an uncommitted file for the
/// very dirty-tree check it exists to support.
async fn lock_path(repo: &Path) -> Result<PathBuf, WorktreeError> {
    let common_dir = run_git(repo, &["rev-parse", "--git-common-dir"]).await?;
    let common_dir = PathBuf::from(common_dir.trim());
    let common_dir = if common_dir.is_absolute() {
        common_dir
    } else {
        repo.join(common_dir)
    };
    Ok(common_dir.join("yunta-none.lock"))
}

async fn lock(repo: &Path) -> Result<(), WorktreeError> {
    use std::io::Write;

    let lock_path = lock_path(repo).await?;
    // `create_new` makes the check-and-create atomic — two processes
    // racing to lock the same repo can't both succeed.
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(mut file) => file.write_all(b"").map_err(|source| WorktreeError::Io {
            action: "create the isolation lock".to_string(),
            path: lock_path,
            source,
        }),
        Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(WorktreeError::Locked {
                path: repo.to_path_buf(),
            })
        }
        Err(source) => Err(WorktreeError::Io {
            action: "create the isolation lock".to_string(),
            path: lock_path,
            source,
        }),
    }
}

async fn run_git(cwd: &Path, args: &[&str]) -> Result<String, WorktreeError> {
    let git_error = |detail: String| WorktreeError::Git {
        args: args.join(" "),
        cwd: cwd.to_path_buf(),
        detail,
    };

    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| git_error(e.to_string()))?;

    if !output.status.success() {
        return Err(git_error(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
