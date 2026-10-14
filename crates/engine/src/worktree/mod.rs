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
//!
//! Waking a run asks the same tree the other question this module
//! answers — whether it is still the tree the run left — and
//! [`WorktreeIntegrity`] is where that lives, together with why the
//! answer is never about the tree's content.

mod branches;
mod capture;
mod commit;
mod error;
mod integrity;
mod listing;
mod old_layouts;
mod overlay;
mod pool;
mod unit;

use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use yunta_core::{CommitSha, InvalidId, Isolation, Pid};

use crate::git::{self, Effect, BRANCH_DELETED};
use crate::lock::{self, Acquired, Contention, LockError, LockOwner, SystemProbe};

use crate::process::Supervision;
pub(crate) use branches::RUN_BRANCHES;
pub(crate) use branches::{is_legacy_unit_branch, legacy_unit_branches_of, LEGACY_UNIT_BRANCHES};
pub use branches::{run_branch, UnitRef};
pub(crate) use capture::capture_for_commit;
pub use capture::{capture_tree, head_tree};
pub use commit::{commit_tree, restore, Captured};
pub use error::{Room, WorktreeError};
pub use integrity::{RunWorktree, WorktreeIntegrity};
pub(crate) use listing::checkouts;
pub use old_layouts::{remove_old_checkouts, OldCheckout};
pub use overlay::{remove_files, tree_with, write_files};
pub use pool::{
    adopt_unit_branches, adoptable_unit_branches, clear_work, forget_attempts, forget_run_units,
    hand_over_run_checkout, pool_home, release_run_checkout, release_unit_checkouts,
    still_the_runs, Birth, CheckoutPool, Lease, PoolReport, Tidied,
};
pub use unit::{
    carry_work, commit_work, land, lands_units, rebase_onto, snapshot_commit, Carried, Rebase,
    Unit, UnitHome, UnitId,
};

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

/// Puts `repo` in the state a run needs before it starts: for
/// `Isolation::Worktree`, creates a dedicated `git worktree` at
/// `worktree_path` on a new branch `branch_name`, checked out at
/// `base_commit`; for `Isolation::None`, verifies `repo` itself is clean
/// and takes its lock (`worktree_path` is ignored — the run operates on
/// `repo` directly).
pub async fn prepare_worktree(
    repo: &Path,
    worktree_path: &Path,
    base_commit: &CommitSha,
    branch_name: &str,
    isolation: Isolation,
    supervision: Supervision<'_>,
) -> Result<WorktreePrepared, WorktreeError> {
    match isolation {
        Isolation::Worktree => {
            add_worktree(repo, worktree_path, base_commit, branch_name, supervision).await
        }
        Isolation::None => {
            if !is_clean(repo, supervision).await? {
                return Err(WorktreeError::DirtyTree {
                    path: repo.to_path_buf(),
                });
            }
            lock(repo, supervision).await
        }
    }
}

/// `prepare_worktree` for `Isolation::Worktree`: the run's linked worktree
/// is added under the worktree-mutation lock, and a cancelled add is undone
/// under that same hold.
async fn add_worktree(
    repo: &Path,
    worktree_path: &Path,
    base_commit: &CommitSha,
    branch_name: &str,
    supervision: Supervision<'_>,
) -> Result<WorktreePrepared, WorktreeError> {
    if let Some(parent) = worktree_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| WorktreeError::Io {
                action: "create the worktrees directory".to_string(),
                path: parent.to_path_buf(),
                source,
            })?;
    }
    let common_dir = git::common_dir(repo, supervision).await?;
    // Held through the add and its undo, so no other mutation of the
    // repository's worktrees runs between them.
    let held = git::mutation_lock(&common_dir, supervision)
        .await
        .map_err(WorktreeError::Lock)?;
    let path = worktree_path.display().to_string();
    let add = [
        "worktree",
        "add",
        path.as_str(),
        "-b",
        branch_name,
        base_commit.as_str(),
    ];
    match git::change_held(&held, repo, &add, supervision).await {
        Ok(_) => Ok(WorktreePrepared::Ready),
        Err(stopped) if stopped.cancelled() => {
            undo_half_added(&held, repo, worktree_path, branch_name, supervision).await;
            Err(stopped.into())
        }
        Err(other) => Err(other.into()),
    }
}

/// Hands what `prepare_worktree` took to `pid`, which drives the run
/// from here on: for `None`, the checkout's lock changes holder. For
/// `Worktree`, a no-op — a run under that isolation works in a tree of
/// its own and shares no lock with anyone.
///
/// What `run --detach` and the control plane's `run_workflow` owe the
/// checkout they just claimed. Both check it is free, create the run,
/// and then leave, and the process they leave behind is the one
/// actually in the tree.
pub async fn hand_over_worktree(
    repo: &Path,
    isolation: Isolation,
    pid: Pid,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    match isolation {
        Isolation::Worktree => Ok(()),
        Isolation::None => {
            let lock_path = lock_path(repo, supervision).await?;
            lock::hand_over(&lock_path, pid, &SystemProbe, supervision.clock).map_err(|source| {
                WorktreeError::Io {
                    action: "hand over the isolation lock".to_string(),
                    path: lock_path,
                    source: match source {
                        LockError::Io { source, .. } => source,
                        other => std::io::Error::other(other.to_string()),
                    },
                }
            })
        }
    }
}

/// Releases what `prepare_worktree` took: for `None`, removes the lock
/// so a later run may proceed. For `Worktree`, a no-op — the worktree
/// stays on disk (see module docs).
pub async fn release_worktree(
    repo: &Path,
    isolation: Isolation,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    match isolation {
        Isolation::Worktree => Ok(()),
        Isolation::None => {
            let lock_path = lock_path(repo, supervision).await?;
            // The lock names the process that holds the checkout, and only
            // that process gives it up. A lock a hand-over moved to another
            // process, or one that is gone, is left as it is.
            match read_lock_owner(&lock_path).await {
                Some(owner) if owner.pid == Pid::current() => {
                    lock::release(&lock_path, &owner).map_err(WorktreeError::Lock)
                }
                _ => Ok(()),
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
    supervision: Supervision<'_>,
) -> Result<WorktreeCleanup, WorktreeError> {
    let git_dir = git::read(
        worktree,
        &["rev-parse", "--path-format=absolute", "--git-dir"],
        supervision,
    )
    .await?;
    let common_dir = git::read(
        worktree,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        supervision,
    )
    .await?;
    if git_dir.trim() == common_dir.trim() {
        return Ok(WorktreeCleanup::NotALinkedWorktree);
    }
    let common = Path::new(common_dir.trim());
    let main_repo = main_repo_of(common)?;

    // Removal rewrites the same `.git/worktrees/` metadata an
    // `add` scans — same lock, same reasoning.
    let held = git::mutation_lock(common, supervision)
        .await
        .map_err(WorktreeError::Lock)?;
    let path = worktree.display().to_string();
    git::change_held(
        &held,
        &main_repo,
        &["worktree", "remove", "--force", path.as_str()],
        supervision,
    )
    .await?;
    // A branch carrying commits nothing merged is refused by `-d`, and
    // that refusal is what protects it: it is not a failure.
    git::attempt_held(
        &held,
        &main_repo,
        &["branch", "-d", branch],
        BRANCH_DELETED,
        supervision,
    )
    .await?;
    Ok(WorktreeCleanup::Removed)
}

/// Drops the metadata of each linked worktree whose directory is gone,
/// under the worktree-mutation lock.
pub async fn prune_worktrees(
    repo: &Path,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    git::change(repo, Effect::Metadata, &["worktree", "prune"], supervision).await?;
    Ok(())
}

/// Removes a linked worktree with `--force` and then prunes its metadata,
/// under one hold of the worktree-mutation lock, so no other worktree
/// mutation runs between the two. The repository is the one the checkout
/// itself names, whichever project asks; git runs in its common
/// directory, which outlives the checkout.
pub async fn remove_checkout(
    checkout: &Path,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let common_dir = git::common_dir(checkout, supervision).await?;
    let held = git::mutation_lock(&common_dir, supervision)
        .await
        .map_err(WorktreeError::Lock)?;
    let path = checkout.display().to_string();
    git::change_held(
        &held,
        &common_dir,
        &["worktree", "remove", "--force", path.as_str()],
        supervision,
    )
    .await?;
    git::change_held(&held, &common_dir, &["worktree", "prune"], supervision).await?;
    Ok(())
}

/// Undoes a `worktree add` a cancellation killed in the middle: the
/// directory it was writing into and the branch it may already have
/// created. Best effort throughout — what it is undoing was killed with
/// no chance to say how far it got, so every step is allowed to find
/// nothing to do.
///
/// It runs under a token of its own: the token that killed the `add`
/// would kill the undo before it started, and the caller asked to stop
/// making a worktree, not to be left with half of one. A bound of its
/// own keeps it from becoming the thing that hangs a Ctrl-C.
pub(super) async fn undo_half_added(
    held: &crate::git::MutationLock,
    repo: &Path,
    worktree_path: &Path,
    branch_name: &str,
    supervision: Supervision<'_>,
) {
    let undo = CancellationToken::new();
    let undo = Supervision::outside_any_run(&undo, supervision.clock)
        .with_env(supervision.env)
        .with_path(supervision.path);
    let path = worktree_path.display().to_string();
    for args in [
        ["worktree", "remove", "--force", &path].as_slice(),
        // A directory git never registered is not a worktree it can
        // remove, and the metadata of one it half-registered is what
        // `prune` is for.
        ["worktree", "prune"].as_slice(),
        // `-D`, not `-d`: this branch was born a moment ago pointing at
        // the base commit, and nothing has been committed on it.
        ["branch", "-D", branch_name].as_slice(),
    ] {
        match tokio::time::timeout(UNDO_BOUND, git::change_held(held, repo, args, undo)).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                tracing::debug!(%error, ?args, "a step of the undo found nothing to undo");
            }
            Err(_) => tracing::debug!(?args, "a step of the undo outlasted its bound"),
        }
    }
    if let Err(error) = tokio::fs::remove_dir_all(worktree_path).await {
        tracing::debug!(%error, path = %worktree_path.display(), "nothing left of the tree to remove");
    }
}

/// How long the undo of a killed `worktree add` may take before it is
/// abandoned: long enough for a local git on a cold
/// cache, short enough that a person who pressed Ctrl-C twice is not
/// waiting on it.
const UNDO_BOUND: std::time::Duration = std::time::Duration::from_secs(10);

/// The commit `repo`'s HEAD is on — the one place the engine asks a
/// checkout where it is, so "HEAD" means the same thing to the batch that
/// branches from it, the promotion that builds on it, and the resume that
/// checks the run's base commit is still behind it.
pub async fn head_commit(
    repo: &Path,
    supervision: Supervision<'_>,
) -> Result<CommitSha, WorktreeError> {
    let output = git::read(repo, &["rev-parse", "HEAD"], supervision).await?;
    output
        .trim()
        .parse()
        .map_err(|source: InvalidId| WorktreeError::NotACommit {
            args: "rev-parse HEAD".to_string(),
            cwd: repo.to_path_buf(),
            source,
        })
}

/// Whether `repo`'s tree is clean, by the one definition `git::tree_state`
/// gives.
async fn is_clean(repo: &Path, supervision: Supervision<'_>) -> Result<bool, WorktreeError> {
    Ok(git::tree_state(repo, &[], supervision).await?.clean)
}

/// The lock `none` isolation takes, in the repository's common git dir:
/// next to git's own metadata rather than inside the working tree, so it
/// never shows up as an uncommitted file for the checks it supports
/// (dirty-tree, scope-by-diff).
async fn lock_path(repo: &Path, supervision: Supervision<'_>) -> Result<PathBuf, WorktreeError> {
    Ok(git::common_dir(repo, supervision)
        .await?
        .join("yunta-none.lock"))
}

/// The holder a lock file names, or `None` when there is no file or
/// nothing in it reads as one.
async fn read_lock_owner(lock_path: &Path) -> Option<LockOwner> {
    let bytes = tokio::fs::read(lock_path).await.ok()?;
    yunta_core::persisted::PersistedDoc::<LockOwner>::read(&bytes)
        .ok()
        .map(|owner| owner.doc)
}

/// Refuses at once on a present holder — a second run on the same
/// checkout is the thing this lock exists to prevent — and reports a
/// takeover from a gone holder, never silently.
async fn lock(
    repo: &Path,
    supervision: Supervision<'_>,
) -> Result<WorktreePrepared, WorktreeError> {
    let lock_path = lock_path(repo, supervision).await?;
    let taken = lock::acquire(
        &lock_path,
        Contention::Refuse,
        &SystemProbe,
        supervision.clock,
        supervision.cancel,
        &|_| false,
    )
    .await;
    match taken {
        Ok(Acquired::Fresh(_)) => Ok(WorktreePrepared::Ready),
        Ok(Acquired::Stolen { dead, .. }) => {
            Ok(WorktreePrepared::StoleStaleLock { dead_pid: dead.pid })
        }
        Err(refused) => Err(error::lock_refusal(repo, refused)),
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
