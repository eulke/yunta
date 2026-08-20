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
//! Cleanup: a prepared worktree is left on disk after the run for
//! inspection by default; a workflow that declares
//! `on_finish.cleanup: worktree` (DI-13) gets [`cleanup_worktree`] at
//! its real Finish instead. `none`'s lock is always released at
//! Finish, since holding it forever would make every run after the
//! first permanently refuse to start.

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
    /// DI-08: a lock whose owner can't be verified — pre-owner-format
    /// (empty) or corrupted. Conservative on purpose: guessing that an
    /// unreadable lock is stale would break the old contract silently.
    #[error(
        "`{path}` has an isolation lock with no readable owner (`{lock_path}`) — written by \
         an older build or corrupted; if no other run is active on this checkout, delete \
         that file by hand and retry"
    )]
    LockedByUnknown { path: PathBuf, lock_path: PathBuf },
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
/// What taking isolation `none`'s lock involved (DI-08) — the caller
/// (CLI) surfaces a takeover to the user; `Worktree` isolation always
/// reports `Ready`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorktreePrepared {
    Ready,
    /// The previous owner was dead — its lock was stolen. Explicit
    /// degradation: report it, never steal silently.
    StoleStaleLock {
        dead_pid: u32,
    },
}

pub async fn prepare_worktree(
    repo: &Path,
    worktree_path: &Path,
    base_commit: &str,
    branch_name: &str,
    isolation: Isolation,
) -> Result<WorktreePrepared, WorktreeError> {
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
            Ok(WorktreePrepared::Ready)
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

/// What [`cleanup_worktree`] did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorktreeCleanup {
    Removed,
    /// The path isn't a *linked* worktree (its git dir is the common
    /// dir) — removing it would delete a primary checkout, so nothing
    /// is touched. The caller reports it; this function never guesses.
    NotALinkedWorktree,
}

/// `on_finish.cleanup: worktree` (§8.3/DI-13): removes the run's linked
/// worktree and then deletes the run branch only if git agrees it's
/// safe (`branch -d`, never `-D`) — a branch still carrying unmerged,
/// unpushed commits (a fresh distill, DI-24) survives, and that is not
/// an error. The worktree removal itself is `--force`: the workflow
/// declared this checkout disposable, and un-committed leftovers are
/// exactly what it wants gone.
pub async fn cleanup_worktree(
    worktree: &Path,
    branch: &str,
) -> Result<WorktreeCleanup, WorktreeError> {
    let git_dir = run_git(
        worktree,
        &["rev-parse", "--path-format=absolute", "--git-dir"],
    )
    .await?;
    let common_dir = run_git(
        worktree,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await?;
    if git_dir.trim() == common_dir.trim() {
        return Ok(WorktreeCleanup::NotALinkedWorktree);
    }
    let main_repo = PathBuf::from(common_dir.trim())
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("/"));

    run_git(
        &main_repo,
        &[
            "worktree",
            "remove",
            "--force",
            &worktree.display().to_string(),
        ],
    )
    .await?;
    // Best-effort by design: `-d` refusing is the branch's protection.
    let _ = run_git(&main_repo, &["branch", "-d", branch]).await;
    Ok(WorktreeCleanup::Removed)
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

/// The lock's content (DI-08): the owning `yunta` process. Liveness is
/// decided by `kill -0` at contention time, never by age — which is why
/// there is no timestamp here.
#[derive(serde::Serialize, serde::Deserialize)]
struct LockOwner {
    pid: u32,
}

async fn lock(repo: &Path) -> Result<WorktreePrepared, WorktreeError> {
    use std::io::Write;

    let lock_path = lock_path(repo).await?;
    let write_err = |source, lock_path| WorktreeError::Io {
        action: "create the isolation lock".to_string(),
        path: lock_path,
        source,
    };
    let owner_json = serde_json::to_string(&LockOwner {
        pid: std::process::id(),
    })
    .map_err(|e| write_err(std::io::Error::other(e), lock_path.clone()))?;

    // `create_new` makes the check-and-create atomic — two processes
    // racing to lock the same repo can't both succeed.
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(mut file) => {
            file.write_all(owner_json.as_bytes())
                .map_err(|source| write_err(source, lock_path))?;
            Ok(WorktreePrepared::Ready)
        }
        Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
            // DI-08: the lock has an owner — is it still alive?
            let owner: Option<LockOwner> = std::fs::read(&lock_path)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok());
            match owner {
                Some(owner) if crate::process_registry::process_alive(owner.pid) => {
                    Err(WorktreeError::Locked {
                        path: repo.to_path_buf(),
                    })
                }
                Some(owner) => {
                    // Dead owner: steal, reported to the caller so the
                    // takeover is explicit, never silent.
                    std::fs::write(&lock_path, owner_json.as_bytes())
                        .map_err(|source| write_err(source, lock_path))?;
                    Ok(WorktreePrepared::StoleStaleLock {
                        dead_pid: owner.pid,
                    })
                }
                None => Err(WorktreeError::LockedByUnknown {
                    path: repo.to_path_buf(),
                    lock_path,
                }),
            }
        }
        Err(source) => Err(write_err(source, lock_path)),
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
