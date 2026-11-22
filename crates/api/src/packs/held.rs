//! See [`super`]. The repository's packs held by one pack command from its
//! read of the lock to its write, through `.yunta/yunta.lock.held` beside the
//! lock, so another one waits rather than writing over an entry it never
//! read.

use std::path::PathBuf;
use std::time::Duration;

use yunta_engine::lock::{self, Acquired, Contention, LockError, LockOwner, SystemProbe};

use super::failed;
use super::vendor::{self, PackError};
use crate::{Error, Failure, Stop, Workspace};

/// How long a pack command waits for another to let go of the packs: the
/// other one is vendoring a pack and writing the lock, which takes seconds,
/// so two minutes means it is hung, not busy.
const HOLD_PATIENCE: Duration = Duration::from_secs(120);

/// How often a waiting pack command tries again: short next to the seconds
/// another one holds the packs, so a hold let go is taken without a visible
/// pause.
const HOLD_POLL: Duration = Duration::from_millis(50);

/// The repository's packs, held by this process: no other pack command reads
/// the lock to write it until this is dropped.
pub(super) struct HeldPacks {
    path: PathBuf,
    owner: LockOwner,
}

impl Drop for HeldPacks {
    fn drop(&mut self) {
        // A `Drop` has no `await` to give, and a release is one read and at
        // most one removal, so it runs here.
        if let Err(error) = lock::release(&self.path, &self.owner) {
            tracing::debug!(%error, lock = %self.path.display(), "the packs were not let go");
        }
    }
}

/// Holds the packs of the repository `workspace` works in, through
/// `.yunta/yunta.lock.held` beside the lock, waiting while another pack
/// command holds them. A holder that is gone is taken over; the caller's stop
/// ends the wait.
pub(super) async fn held(workspace: &Workspace, stop: &Stop) -> Result<HeldPacks, Error> {
    let path = vendor::hold_path(&workspace.dir);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|source| {
            failed(PackError::Write {
                path: parent.to_path_buf(),
                source,
            })
        })?;
    }
    let contention = Contention::Wait {
        patience: HOLD_PATIENCE,
        poll: HOLD_POLL,
    };
    let clock = workspace.clock.as_ref();
    let taken = lock::acquire(&path, contention, &SystemProbe, clock, stop.work(), &|_| {
        false
    });
    match taken.await {
        Ok(Acquired::Fresh(owner) | Acquired::Stolen { owner, .. }) => {
            Ok(HeldPacks { path, owner })
        }
        Err(LockError::Cancelled { .. }) => Err(Error::Stopped),
        Err(error) => Err(Failure::new(error).into()),
    }
}
