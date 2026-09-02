//! A lock file owned by one process, honest about who holds it. The
//! file's content is the holder's [`LockOwner`]; taking the lock is one
//! atomic `create_new`; a holder that is gone is replaced by removing
//! its file and creating anew, never by writing over it — two
//! stealers racing both go through `create_new` again, and exactly one
//! wins. "Gone" is the probe's answer, and a holder whose state the
//! probe cannot tell counts as present: a run must never lose the
//! exclusivity it still holds without a sound.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use yunta_adapters::signal::{self, Liveness};
use yunta_core::{Clock, Pid};

/// What the host says about a lock's holder.
pub trait OwnerProbe {
    fn liveness(&self, pid: Pid) -> Liveness;

    /// When the process with `pid` started, where the host can tell.
    fn started(&self, pid: Pid) -> Option<DateTime<Utc>>;
}

/// The host's own process table.
pub struct SystemProbe;

impl OwnerProbe for SystemProbe {
    fn liveness(&self, pid: Pid) -> Liveness {
        signal::liveness(pid)
    }

    fn started(&self, pid: Pid) -> Option<DateTime<Utc>> {
        yunta_adapters::process_start::process_start(pid).map(DateTime::from)
    }
}

/// The lock's content: who holds it and since when. The time tells a
/// reused pid from the holder — a process that started after the lock
/// was taken is not the one that took it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockOwner {
    pub pid: Pid,
    pub started_at: DateTime<Utc>,
}

/// What to do while another process holds the lock.
#[derive(Debug, Clone, Copy)]
pub enum Contention {
    /// Report the holder at once.
    Refuse,
    /// Retry every `poll` until `patience` runs out, then report.
    Wait { patience: Duration, poll: Duration },
}

/// How the lock was taken.
#[derive(Debug)]
pub enum Acquired {
    Fresh,
    /// The previous holder was gone; its record, for the report.
    Stolen {
        dead: LockOwner,
    },
}

#[derive(Debug, Error)]
pub enum LockError {
    /// A holder the probe reports alive, or one it cannot ask about.
    #[error("`{lock_path}` is held by pid {}", owner.pid)]
    Held {
        lock_path: PathBuf,
        owner: LockOwner,
        liveness: Liveness,
    },
    /// The file exists but names no holder: written by an older build,
    /// corrupted, or caught between its holder's `create_new` and its
    /// write.
    #[error("`{lock_path}` exists but names no readable holder")]
    Unreadable { lock_path: PathBuf },
    /// The patience ran out while a holder kept the lock.
    #[error(
        "gave up waiting for `{lock_path}`{}",
        owner.as_ref().map(|o| format!(" (held by pid {})", o.pid)).unwrap_or_default()
    )]
    Timeout {
        lock_path: PathBuf,
        owner: Option<LockOwner>,
    },
    #[error("failed to {action} `{lock_path}`")]
    Io {
        action: &'static str,
        lock_path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Takes the lock at `lock_path` for this process, recording it as the
/// holder with the clock's time. A gone holder (dead, or a live process
/// that started after the lock was taken and so merely reuses the pid)
/// is stolen; a present one is refused or waited on per `contention`.
pub async fn acquire(
    lock_path: &Path,
    contention: Contention,
    probe: &dyn OwnerProbe,
    clock: &dyn Clock,
) -> Result<Acquired, LockError> {
    let io = |action: &'static str, source| LockError::Io {
        action,
        lock_path: lock_path.to_path_buf(),
        source,
    };
    let deadline = match contention {
        Contention::Refuse => None,
        Contention::Wait { patience, .. } => Some(Instant::now() + patience),
    };
    let mut stolen_from = None;
    loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(mut file) => {
                let record = LockOwner {
                    pid: Pid::current(),
                    started_at: clock.now(),
                };
                let json = serde_json::to_string(&record)
                    .map_err(|e| io("encode the holder of", std::io::Error::other(e)))?;
                file.write_all(json.as_bytes())
                    .map_err(|source| io("write the holder of", source))?;
                return Ok(match stolen_from {
                    Some(dead) => Acquired::Stolen { dead },
                    None => Acquired::Fresh,
                });
            }
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
                let owner: Option<LockOwner> = std::fs::read(lock_path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok());
                let liveness = owner.as_ref().map(|owner| holder_state(owner, probe));
                if let (Some(owner), Some(Liveness::Dead)) = (&owner, liveness) {
                    // Remove, then retry: the atomic `create_new` above
                    // decides which of two concurrent stealers wins.
                    match std::fs::remove_file(lock_path) {
                        Ok(()) => {}
                        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                        Err(source) => return Err(io("remove the stale", source)),
                    }
                    stolen_from = Some(owner.clone());
                    continue;
                }
                // Alive, unknown, or unreadable: present.
                match (deadline, contention) {
                    (Some(deadline), Contention::Wait { poll, .. })
                        if Instant::now() < deadline =>
                    {
                        tokio::time::sleep(poll).await;
                    }
                    (Some(_), _) => {
                        return Err(LockError::Timeout {
                            lock_path: lock_path.to_path_buf(),
                            owner,
                        });
                    }
                    (None, _) => {
                        return Err(match (owner, liveness) {
                            (Some(owner), Some(liveness)) => LockError::Held {
                                lock_path: lock_path.to_path_buf(),
                                owner,
                                liveness,
                            },
                            _ => LockError::Unreadable {
                                lock_path: lock_path.to_path_buf(),
                            },
                        });
                    }
                }
            }
            Err(source) => return Err(io("create", source)),
        }
    }
}

/// Dead when the probe says so, or when the live process with that pid
/// started after the lock was taken: a newcomer that got the holder's
/// pid, not the holder.
fn holder_state(owner: &LockOwner, probe: &dyn OwnerProbe) -> Liveness {
    match probe.liveness(owner.pid) {
        Liveness::Alive => match probe.started(owner.pid) {
            Some(started) if started > owner.started_at => Liveness::Dead,
            _ => Liveness::Alive,
        },
        other => other,
    }
}
