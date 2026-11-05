//! The claim that one process drives a run: a second engine would write the same
//! log and work in the same checkout. Not the isolation lock, which claims a
//! checkout: a run can change checkouts, and this claim stays with the run.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use yunta_core::Clock;

use crate::lock::{self, Acquired, Contention, LockError, LockOwner, SystemProbe};
use crate::replay::RunState;

/// How long a parked run's next engine waits for the engine that parked it: that
/// engine leaves moments after it writes the pause, and a longer wait would only
/// hide one that did not.
const LEAVING: Duration = Duration::from_secs(5);

/// How often that engine looks again: short next to the wait, so a run let go is
/// taken without a visible pause.
const LEAVING_POLL: Duration = Duration::from_millis(15);

/// Proof that this process drives a run: `RunEnv` borrows one and only
/// [`hold_run`] makes one, so no run is driven without it. Released on drop; a
/// process that dies without dropping it is replaced by the next holder.
#[derive(Debug)]
#[must_use]
pub struct RunHold {
    run_dir: PathBuf,
    owner: LockOwner,
}

impl RunHold {
    /// The directory of the run this hold claims.
    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }
}

impl Drop for RunHold {
    fn drop(&mut self) {
        let path = crate::run_dir::engine_lock(&self.run_dir);
        if let Err(error) = lock::release(&path, &self.owner) {
            tracing::warn!(%error, "failed to release a run's engine lock");
        }
    }
}

/// Claims the run at `run_dir` for this process. A holder that is gone is replaced
/// and never kept: a resume already answers for the groups it left running.
pub async fn hold_run(
    run_dir: &Path,
    contention: Contention,
    clock: &dyn Clock,
    cancel: &CancellationToken,
) -> Result<RunHold, LockError> {
    let path = crate::run_dir::engine_lock(run_dir);
    if let Some(scratch) = path.parent() {
        tokio::fs::create_dir_all(scratch)
            .await
            .map_err(|source| LockError::Io {
                action: "create the directory of",
                lock_path: path.clone(),
                source,
            })?;
    }
    let (Acquired::Fresh(owner) | Acquired::Stolen { owner, .. }) =
        lock::acquire(&path, contention, &SystemProbe, clock, cancel, &|_| false).await?;
    Ok(RunHold {
        run_dir: run_dir.to_path_buf(),
        owner,
    })
}

/// What a hold on `run_dir` would be refused with right now, taking nothing.
pub async fn hold_refused(run_dir: &Path) -> Option<LockError> {
    lock::refusal_now(&crate::run_dir::engine_lock(run_dir), &SystemProbe).await
}

/// How to contend for a run the log reads as `state`: the engine that parked it
/// is on its way out, and one still moving is not coming back soon. Parked is the
/// run's pause, which only a resume ends, and not the log's last event: an answer
/// recorded after the pause can reach the log while that engine is still leaving.
pub fn contention_for(state: &RunState) -> Contention {
    match state.run.paused() {
        Some(_) => Contention::Wait {
            patience: LEAVING,
            poll: LEAVING_POLL,
        },
        None => Contention::Refuse,
    }
}
