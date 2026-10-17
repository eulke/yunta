//! What a worktree operation can fail with: one error for the module, with
//! the git and lock failures it carries, and the disk room a checkout needs.

use std::path::{Path, PathBuf};

use thiserror::Error;
use yunta_core::process::signal::Liveness;
use yunta_core::{CommitSha, InvalidId, Pid};

use crate::git::Cause;
use crate::lock::LockError;

#[derive(Debug, Error)]
pub enum WorktreeError {
    #[error(transparent)]
    Git(crate::git::GitError),
    /// A lock the operation needed and did not get: a holder kept it past
    /// the wait, the token stopped the wait, or the lock file could not be
    /// read or written. A holder's pid comes with the error when there is one.
    #[error(transparent)]
    Lock(LockError),
    /// A commit the operation was asked about is not in the repository it
    /// was asked of.
    #[error("the commit `{commit}` is not in this repository")]
    UnknownCommit { commit: CommitSha },
    /// git answered, and its answer is not what it names. `rev-parse`
    /// promised a commit sha and returned something else — a broken git,
    /// not a failed command.
    #[error("git answered `{args}` in `{}` with something that is not a commit sha", .cwd.display())]
    NotACommit {
        args: String,
        cwd: PathBuf,
        #[source]
        source: InvalidId,
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
    /// A lock whose owner can't be verified — empty or corrupted.
    /// Conservative on purpose: a lock whose holder cannot be checked is
    /// never taken, so no live run loses its checkout to a guess.
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
    /// The disk has no room for another build without going under the
    /// part of it the pool always leaves free, even with every build the
    /// pool may empty emptied.
    #[error(
        "the disk has {} free and the checkout needs about {} more, where {} stays free",
        yunta_core::units::Bytes(.room.available),
        yunta_core::units::Bytes(.room.need),
        yunta_core::units::Bytes(.room.keep_free)
    )]
    NoRoom { room: Room },
    /// A checkout of the project's pool a unit went back to is held by
    /// another process, which did not let go of it in the moment given.
    #[error("the checkout `{}` is held by another process", .path.display())]
    CheckoutHeld { path: PathBuf },
    /// A run's own linked worktree is not at the path its manifest
    /// froze — moved, deleted, or replaced by something git does not
    /// know as a working tree. Not a broken run: the log and the objects
    /// that are the run's evidence are untouched, and git still holds the
    /// run's branch, so the checkout comes back with one command.
    #[error(
        "the run's branch `{branch}` has no worktree at `{path}` ({detail}) — the run's \
         history and artifacts are intact; bring the checkout back with `git worktree add \
         {path} {branch}`, run from the repository the run was created in, and resume again"
    )]
    RunWorktreeLost {
        path: PathBuf,
        branch: String,
        detail: String,
    },
    /// A run under isolation `none` works on the checkout it was created
    /// in, and `resume` was run somewhere that is not one.
    #[error(
        "`{path}` is not a git working tree ({detail}) — this run has `isolation: none`, so it \
         works directly on the checkout it was created in; resume it from there"
    )]
    NotACheckout { path: PathBuf, detail: String },
    /// git answered, and its answer is not what it names: `rev-parse`
    /// promised a tree and returned something else.
    #[error("git answered `{args}` in `{}` with something that is not a tree id", .cwd.display())]
    NotATree {
        args: String,
        cwd: PathBuf,
        #[source]
        source: InvalidId,
    },
    /// The shared tree moved between a unit replaying its work onto it
    /// and that unit landing. This engine is the only writer of that
    /// tree in between, so the invariant broke rather than the unit
    /// misbehaving.
    #[error(
        "landing `{unit}` in `{}`: git refused a fast-forward onto a head this engine is \
         the only writer of",
        .path.display()
    )]
    NotFastForward { unit: String, path: PathBuf },
    /// The common git dir has no parent directory, so there is no
    /// checkout to run `git worktree` from.
    #[error("the git common dir `{common_dir}` has no parent directory to run `git worktree` in")]
    NoMainRepo { common_dir: PathBuf },
}

/// What a checkout needed of the disk and did not find.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Room {
    pub available: u64,
    pub need: u64,
    pub keep_free: u64,
}

impl Room {
    /// Why a run stops for it.
    pub fn pause(self) -> yunta_core::events::PauseReason {
        yunta_core::events::PauseReason::NoRoom {
            available: self.available,
            need: self.need,
            keep_free: self.keep_free,
        }
    }
}

impl From<crate::git::GitError> for WorktreeError {
    /// A lock the token stopped, or a holder the wait gave up on, reaches
    /// the caller as the lock's own error, with the holder's pid. It is
    /// never a failed git.
    fn from(error: crate::git::GitError) -> Self {
        match error.cause {
            Cause::Lock(lock) => WorktreeError::Lock(*lock),
            cause => WorktreeError::Git(crate::git::GitError {
                invocation: error.invocation,
                cause,
            }),
        }
    }
}

impl WorktreeError {
    /// Whether the token this ran under fired: nobody is waiting for
    /// the tree any more, so there is no failure to report — only a
    /// stop to honour.
    pub fn cancelled(&self) -> bool {
        match self {
            WorktreeError::Git(error) => error.cancelled(),
            WorktreeError::Lock(lock) => matches!(lock, LockError::Cancelled { .. }),
            _ => false,
        }
    }
}

/// What a refused isolation lock means to a run on `repo`: a holder that
/// may still be running, one whose owner cannot be told, or a wait the
/// token stopped.
pub(super) fn lock_refusal(repo: &Path, refused: LockError) -> WorktreeError {
    let path = repo.to_path_buf();
    match refused {
        LockError::Held {
            lock_path,
            owner,
            liveness: Liveness::Unknown,
        } => WorktreeError::LockedByAnotherUser {
            path,
            lock_path,
            pid: owner.pid,
        },
        LockError::Held { owner, .. } | LockError::Kept { owner, .. } => WorktreeError::Locked {
            path,
            pid: owner.pid,
        },
        LockError::Timeout {
            owner: Some(owner), ..
        } => WorktreeError::Locked {
            path,
            pid: owner.pid,
        },
        LockError::Timeout {
            owner: None,
            lock_path,
        }
        | LockError::Unreadable { lock_path } => WorktreeError::LockedByUnknown { path, lock_path },
        cancelled @ LockError::Cancelled { .. } => WorktreeError::Lock(cancelled),
        LockError::Io {
            action,
            lock_path,
            source,
        } => WorktreeError::Io {
            action: format!("{action} the isolation lock"),
            path: lock_path,
            source,
        },
    }
}
