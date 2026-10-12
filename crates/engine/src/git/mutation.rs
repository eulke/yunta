//! The worktree-mutation lock: `yunta-worktree.lock` in a common git
//! directory, taken by a caller for the calls that change which checkouts
//! exist. A live holder is waited on; the token stops the wait; a holder
//! that is gone is taken over.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::lock::{self, Acquired, Contention, LockError, LockOwner, SystemProbe};
use crate::process::Supervision;

use super::repo::common_dir;
use super::{error_for, Cause, GitError};

/// Holds `yunta-worktree.lock` for one caller. Dropping it releases the
/// lock through `lock::release`, and logs a failure at debug, as the
/// drop of a checkout's lease does: a `Drop` cannot return an error.
///
/// A caller that already holds one uses [`super::change_held`] or
/// [`super::attempt_held`]: a [`super::change`] with
/// [`super::Effect::Metadata`] would wait on the very lock the caller holds,
/// and give up when its patience ran out.
pub struct MutationLock {
    path: PathBuf,
    owner: LockOwner,
}

impl Drop for MutationLock {
    fn drop(&mut self) {
        // A `Drop` has no `await` to give, and a release is one read and at
        // most one removal, so it runs here.
        if let Err(error) = lock::release(&self.path, &self.owner) {
            tracing::debug!(%error, lock = %self.path.display(), "the worktree-mutation lock was not released");
        }
    }
}

/// How long a waiter keeps trying against a live holder before it gives up
/// loudly. Worktree mutations take tens of milliseconds, so thirty seconds
/// of patience means the holder is hung, not busy.
const MUTATION_LOCK_TIMEOUT: Duration = Duration::from_secs(30);
/// How often a waiter tries again. It is short next to a mutation's tens of
/// milliseconds, so a released lock is taken without a visible pause.
const MUTATION_LOCK_POLL: Duration = Duration::from_millis(15);

/// Takes `yunta-worktree.lock` in the common git directory `common_dir`. A
/// live holder is waited on for at most `MUTATION_LOCK_TIMEOUT`; the token
/// that stops the call stops the wait as well, and that is a
/// [`LockError::Cancelled`].
pub async fn mutation_lock(
    common_dir: &Path,
    sup: Supervision<'_>,
) -> Result<MutationLock, LockError> {
    let path = common_dir.join("yunta-worktree.lock");
    let contention = Contention::Wait {
        patience: MUTATION_LOCK_TIMEOUT,
        poll: MUTATION_LOCK_POLL,
    };
    let owner = match lock::acquire(
        &path,
        contention,
        &SystemProbe,
        sup.clock,
        sup.cancel,
        &|_| false,
    )
    .await?
    {
        Acquired::Fresh(owner) => owner,
        Acquired::Stolen { owner, dead } => {
            // Taking over a dead holder's lock is the protocol working as
            // designed: a crash mid-mutation is exactly what leaves one
            // behind, so it is worth a log line and no more.
            tracing::info!(
                pid = %dead.pid,
                since = %dead.started_at,
                "took over the worktree-mutation lock of a process that is gone"
            );
            owner
        }
    };
    Ok(MutationLock { path, owner })
}

/// The mutation lock a metadata call in the repository `dir` belongs to. A
/// lock that was not got is the call's error, as its cause.
pub(super) async fn held_for(
    dir: &Path,
    args: &[&str],
    sup: Supervision<'_>,
) -> Result<MutationLock, GitError> {
    let common = common_dir(dir, sup).await?;
    mutation_lock(&common, sup)
        .await
        .map_err(|error| error_for(dir, args, Cause::Lock(Box::new(error))))
}
