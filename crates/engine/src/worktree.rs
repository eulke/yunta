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
//! on that same repo via a lock file next to git's own metadata. Both
//! locks this module takes — the `none` isolation lock and the
//! worktree-mutation lock — are [`crate::lock`] files: one protocol,
//! one owner record, one notion of a holder being gone.
//!
//! Cleanup: a prepared worktree is left on disk after the run for
//! inspection by default; a workflow that declares
//! `on_finish.cleanup: worktree` gets [`cleanup_worktree`] at
//! its real Finish instead. `none`'s lock is always released at
//! Finish, since holding it forever would make every run after the
//! first permanently refuse to start.

use std::path::{Path, PathBuf};

use thiserror::Error;
use yunta_adapters::signal::Liveness;
use yunta_core::{Isolation, Pid, SystemClock};

use crate::lock::{self, Acquired, Contention, LockError, SystemProbe};
use yunta_core::CommitSha;

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
        "`{path}` already has a run in progress under isolation `none` (pid {pid}) — only one \
         at a time is allowed on the same checkout; use isolation `worktree` to run concurrently"
    )]
    Locked { path: PathBuf, pid: Pid },
    /// A lock whose holder this process cannot ask about — it runs as
    /// another user. Whether it is still a run on this checkout cannot
    /// be told from here, so the lock is never taken from it.
    #[error(
        "`{path}` has an isolation lock held by pid {pid}, a process this user cannot signal \
         (`{lock_path}`) — if no run is active on this checkout, delete that file by hand and \
         retry"
    )]
    LockedByAnotherUser {
        path: PathBuf,
        lock_path: PathBuf,
        pid: Pid,
    },
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
        owner_pid: Option<Pid>,
    },
    /// The common git dir has no parent directory, so there is no
    /// checkout to run `git worktree` from.
    #[error("the git common dir `{common_dir}` has no parent directory to run `git worktree` in")]
    NoMainRepo { common_dir: PathBuf },
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
        dead_pid: Pid,
    },
}

pub async fn prepare_worktree(
    repo: &Path,
    worktree_path: &Path,
    base_commit: &CommitSha,
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
                    base_commit.as_str(),
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
    let main_repo = main_repo_of(Path::new(common_dir.trim()))?;

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
/// Waits its patience out on a present holder; a gone holder's lock is
/// taken over, and said so, since a crash mid-mutation is exactly what
/// leaves one behind.
async fn lock_worktree_mutations(
    common_dir: &Path,
) -> Result<WorktreeMutationGuard, WorktreeError> {
    let lock_path = common_dir.join("yunta-worktree.lock");
    let contention = Contention::Wait {
        patience: MUTATION_LOCK_TIMEOUT,
        poll: MUTATION_LOCK_POLL,
    };
    match lock::acquire(&lock_path, contention, &SystemProbe, &SystemClock).await {
        Ok(Acquired::Fresh) => Ok(WorktreeMutationGuard { lock_path }),
        Ok(Acquired::Stolen { dead }) => {
            // Taking over a dead holder's lock is the protocol working as
            // designed, not a degradation of this run — a crash
            // mid-mutation is exactly what leaves one behind. Worth a log
            // line, not a warning.
            tracing::info!(
                pid = %dead.pid,
                since = %dead.started_at,
                "took over the worktree-mutation lock of a process that is gone"
            );
            Ok(WorktreeMutationGuard { lock_path })
        }
        Err(LockError::Timeout { lock_path, owner }) => Err(WorktreeError::MutationLockTimeout {
            lock_path,
            owner_pid: owner.map(|owner| owner.pid),
        }),
        Err(LockError::Held {
            lock_path, owner, ..
        }) => Err(WorktreeError::MutationLockTimeout {
            lock_path,
            owner_pid: Some(owner.pid),
        }),
        Err(LockError::Unreadable { lock_path }) => Err(WorktreeError::MutationLockTimeout {
            lock_path,
            owner_pid: None,
        }),
        Err(LockError::Io {
            action,
            lock_path,
            source,
        }) => Err(WorktreeError::Io {
            action: format!("{action} the worktree-mutation lock"),
            path: lock_path,
            source,
        }),
    }
}

/// Refuses at once on a present holder — a second run on the same
/// checkout is the thing this lock exists to prevent — and reports a
/// takeover from a gone holder, never silently.
async fn lock(repo: &Path) -> Result<WorktreePrepared, WorktreeError> {
    let lock_path = lock_path(repo).await?;
    match lock::acquire(&lock_path, Contention::Refuse, &SystemProbe, &SystemClock).await {
        Ok(Acquired::Fresh) => Ok(WorktreePrepared::Ready),
        Ok(Acquired::Stolen { dead }) => {
            Ok(WorktreePrepared::StoleStaleLock { dead_pid: dead.pid })
        }
        Err(LockError::Held {
            lock_path,
            owner,
            liveness: Liveness::Unknown,
        }) => Err(WorktreeError::LockedByAnotherUser {
            path: repo.to_path_buf(),
            lock_path,
            pid: owner.pid,
        }),
        Err(LockError::Held { owner, .. }) => Err(WorktreeError::Locked {
            path: repo.to_path_buf(),
            pid: owner.pid,
        }),
        Err(LockError::Timeout { owner, .. }) => match owner {
            Some(owner) => Err(WorktreeError::Locked {
                path: repo.to_path_buf(),
                pid: owner.pid,
            }),
            None => Err(WorktreeError::LockedByUnknown {
                path: repo.to_path_buf(),
                lock_path,
            }),
        },
        Err(LockError::Unreadable { lock_path }) => Err(WorktreeError::LockedByUnknown {
            path: repo.to_path_buf(),
            lock_path,
        }),
        Err(LockError::Io {
            action,
            lock_path,
            source,
        }) => Err(WorktreeError::Io {
            action: format!("{action} the isolation lock"),
            path: lock_path,
            source,
        }),
    }
}

/// The main checkout of the repo whose common git dir is `common_dir`:
/// the directory that contains it.
fn main_repo_of(common_dir: &Path) -> Result<PathBuf, WorktreeError> {
    common_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| WorktreeError::NoMainRepo {
            common_dir: common_dir.to_path_buf(),
        })
}

async fn run_git(cwd: &Path, args: &[&str]) -> Result<String, WorktreeError> {
    crate::git::output(cwd, args).await.map_err(|e| {
        let detail = e.detail();
        WorktreeError::Git {
            args: e.args,
            cwd: e.cwd,
            detail,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_common_dir_without_a_parent_names_no_main_repo() {
        let err = main_repo_of(Path::new("/")).unwrap_err();
        assert!(
            matches!(err, WorktreeError::NoMainRepo { .. }),
            "got: {err:?}"
        );
        assert_eq!(
            main_repo_of(Path::new("/srv/repo/.git")).unwrap(),
            PathBuf::from("/srv/repo")
        );
    }
}
