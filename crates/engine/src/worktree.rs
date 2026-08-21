//! Working-tree isolation.
//!
//! `worktree` (default): each run gets its own `git worktree`, checked
//! out from the manifest's frozen `base_commit` — concurrent runs on the
//! same repo never collide, and the user's own checkout stays untouched
//! by the agent's edits. `none`: the run operates directly on the given
//! checkout — legitimate for watching an agent edit live, or CI already
//! inside an ephemeral container — but the engine **requires a clean
//! tree** at the start (without that, scope-by-diff can't tell
//! the agent's work from the user's) and refuses a second concurrent run
//! on that same repo via a lock file next to git's own metadata.
//!
//! Cleanup: a prepared worktree is left on disk after the run for
//! inspection by default; a workflow that declares
//! `on_finish.cleanup: worktree` gets [`cleanup_worktree`] at
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
        "`{path}` has uncommitted changes — isolation `none` requires a clean tree: \
         without it, scope-by-diff can't tell the agent's work from what was already there"
    )]
    DirtyTree { path: PathBuf },
    #[error(
        "`{path}` already has a run in progress under isolation `none` — only one at a time \
         is allowed on the same checkout; use isolation `worktree` to run concurrently"
    )]
    Locked { path: PathBuf },
    /// A lock whose owner can't be verified — pre-owner-format
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
    /// Another process has held the worktree-mutation lock past
    /// the bounded wait — never silently proceed into the race the lock
    /// exists to prevent.
    #[error(
        "gave up waiting for the worktree-mutation lock `{lock_path}`{owner} — another \
         process is mutating this repo's worktrees; if it's hung, stop it (or delete the \
         lock by hand once you're sure nothing is running) and retry",
        owner = .owner_pid.map(|pid| format!(" (held by pid {pid})")).unwrap_or_default()
    )]
    MutationLockTimeout {
        lock_path: PathBuf,
        owner_pid: Option<u32>,
    },
}

/// Puts `repo` in the state a run needs before it starts: for
/// `Isolation::Worktree`, creates a dedicated `git worktree` at
/// `worktree_path` on a new branch `branch_name`, checked out at
/// `base_commit`; for `Isolation::None`, verifies `repo` itself is clean
/// and takes its lock (`worktree_path` is ignored — the run operates on
/// `repo` directly).
/// What taking isolation `none`'s lock involved — the caller
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
            let common_dir = common_git_dir(repo).await?;
            let _mutation_lock = lock_worktree_mutations(&common_dir).await?;
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

/// `on_finish.cleanup: worktree`: removes the run's linked
/// worktree and then deletes the run branch only if git agrees it's
/// safe (`branch -d`, never `-D`) — a branch still carrying unmerged,
/// unpushed commits (a fresh distill) survives, and that is not
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

    // Removal rewrites the same `.git/worktrees/` metadata an
    // `add` scans — same lock, same reasoning.
    let _mutation_lock = lock_worktree_mutations(Path::new(common_dir.trim())).await?;
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

/// The repo's common git dir (`--git-common-dir`, correct even when
/// `repo` is itself already a worktree) — where both of Yunta's lock
/// files live, next to git's own metadata rather than inside the
/// working tree: they must never show up as uncommitted files for the
/// very checks they exist to support (dirty-tree, scope-by-diff).
async fn common_git_dir(repo: &Path) -> Result<PathBuf, WorktreeError> {
    let common_dir = run_git(repo, &["rev-parse", "--git-common-dir"]).await?;
    let common_dir = PathBuf::from(common_dir.trim());
    Ok(if common_dir.is_absolute() {
        common_dir
    } else {
        repo.join(common_dir)
    })
}

async fn lock_path(repo: &Path) -> Result<PathBuf, WorktreeError> {
    Ok(common_git_dir(repo).await?.join("yunta-none.lock"))
}

/// How long an acquirer waits on a live holder before giving up
/// loudly. Worktree mutations take tens of milliseconds — 30s of
/// patience means the holder is hung, not busy.
const MUTATION_LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const MUTATION_LOCK_POLL: std::time::Duration = std::time::Duration::from_millis(15);

/// Holds `yunta-worktree.lock` for the duration of one `git
/// worktree` mutation; dropping it releases. Removal in `Drop` (not an
/// explicit method) so an early `?` return can't leak the lock.
struct WorktreeMutationGuard {
    lock_path: PathBuf,
}

impl Drop for WorktreeMutationGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

/// Serializes every `git worktree` mutation on one repo —
/// in-process *and* cross-process. Git mutates `.git/worktrees/`
/// without a complete lock between `add`s, so N concurrent additions
/// (a `concurrency: N` task batch, two `kind: workflow` nodes in one
/// scheduler batch, two MCP `run_workflow` calls) can read each
/// other's half-written metadata and fail with `failed to read
/// .git/worktrees/<x>/commondir`. One file lock in the common git dir
/// — the same home as `yunta-none.lock`, shared by every linked
/// worktree of the repo, invisible to scope-by-diff — covers all of it,
/// and living inside this module's only mutation functions means no
/// call site can forget it.
///
/// Same owner model as the `none` lock: content is the holder's
/// pid, liveness by `kill -0` at contention time. A dead holder's lock
/// is stolen by *removing* it and retrying the atomic `create_new` —
/// never by overwriting in place, which would let two stealers both
/// think they won.
async fn lock_worktree_mutations(
    common_dir: &Path,
) -> Result<WorktreeMutationGuard, WorktreeError> {
    use std::io::Write;

    let lock_path = common_dir.join("yunta-worktree.lock");
    let write_err = |source, lock_path| WorktreeError::Io {
        action: "create the worktree-mutation lock".to_string(),
        path: lock_path,
        source,
    };
    let owner_json = serde_json::to_string(&LockOwner {
        pid: std::process::id(),
    })
    .map_err(|e| write_err(std::io::Error::other(e), lock_path.clone()))?;

    let deadline = std::time::Instant::now() + MUTATION_LOCK_TIMEOUT;
    loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                file.write_all(owner_json.as_bytes())
                    .map_err(|source| write_err(source, lock_path.clone()))?;
                return Ok(WorktreeMutationGuard { lock_path });
            }
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
                let owner: Option<LockOwner> = std::fs::read(&lock_path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok());
                match owner {
                    Some(owner) if !crate::process_registry::process_alive(owner.pid) => {
                        // Dead holder: steal by remove-then-retry — the
                        // atomic `create_new` above decides which of two
                        // concurrent stealers actually wins.
                        let _ = std::fs::remove_file(&lock_path);
                        continue;
                    }
                    // Alive, or unreadable (a holder between its
                    // `create_new` and its `write_all` — microseconds):
                    // wait our turn.
                    _ => {}
                }
                if std::time::Instant::now() >= deadline {
                    return Err(WorktreeError::MutationLockTimeout {
                        lock_path,
                        owner_pid: owner.map(|o| o.pid),
                    });
                }
                tokio::time::sleep(MUTATION_LOCK_POLL).await;
            }
            Err(source) => return Err(write_err(source, lock_path)),
        }
    }
}

/// The lock's content: the owning `yunta` process. Liveness is
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
            // The lock has an owner — is it still alive?
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
