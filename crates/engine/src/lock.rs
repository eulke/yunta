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
use yunta_core::process::signal::{self, Liveness};
use yunta_core::{Clock, Pid};

/// What the host says about a lock's holder. `Send + Sync` so a
/// `&dyn OwnerProbe` can be held across the awaits in [`acquire`] inside a
/// `Send` future — the in-process control plane prepares a worktree from
/// one.
pub trait OwnerProbe: Send + Sync {
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
        yunta_core::process::process_start::process_start(pid).map(DateTime::from)
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

/// Hands the lock at `lock_path` to `pid`, which takes over as its
/// holder from here on.
///
/// A lock is a claim on a checkout by whoever is working in it, and
/// there is one shape of command where those stop being the same
/// process: one that checks the checkout is free, creates a run, and
/// hands it to a process of its own to drive. The claim moves with the
/// work. A lock left naming the process that did the checking reads as
/// a dead holder the moment it exits, and the next run steals a
/// checkout somebody is working in.
///
/// Written over rather than re-created, because this process holds the
/// lock while it writes: the remove-then-`create_new` dance in
/// [`acquire`] is how a *stealer* avoids racing another stealer, and a
/// holder handing its own lock on races nobody.
pub fn hand_over(
    lock_path: &Path,
    pid: Pid,
    probe: &dyn OwnerProbe,
    clock: &dyn Clock,
) -> Result<(), LockError> {
    let io = |action: &'static str, source| LockError::Io {
        action,
        lock_path: lock_path.to_path_buf(),
        source,
    };
    let record = LockOwner {
        pid,
        started_at: taken_at(pid, probe, clock),
    };
    let json = serde_json::to_string(&record)
        .map_err(|e| io("encode the holder of", std::io::Error::other(e)))?;
    // blocking: `hand_over` names a file's owner before the process it
    // names exists, on the caller's own thread and outside any run.
    std::fs::write(lock_path, json).map_err(|source| io("write the holder of", source))
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
        // blocking: taking a lock is `create_new` — the one operation
        // whose atomicity is the whole mechanism, and which `tokio::fs`
        // would run on a pool thread without making any more atomic.
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(mut file) => {
                let pid = Pid::current();
                let record = LockOwner {
                    pid,
                    started_at: taken_at(pid, probe, clock),
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
                let owner: Option<LockOwner> = tokio::fs::read(lock_path)
                    .await
                    .ok()
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok());
                let liveness = owner.as_ref().map(|owner| holder_state(owner, probe));
                if let (Some(owner), Some(Liveness::Dead)) = (&owner, liveness) {
                    // Remove, then retry: the atomic `create_new` above
                    // decides which of two concurrent stealers wins.
                    match tokio::fs::remove_file(lock_path).await {
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
/// When the process a lock names started, as the record has to state
/// it: read from the same process table [`holder_state`] compares it
/// against.
///
/// Not the clock's `now()`. The comparison that decides whether a pid
/// was reused reads the host's process table, and a run's clock answers
/// about the run's timeline — a frozen one, or one an hour behind the
/// host, would make every live holder read as a stranger and every lock
/// stealable. The clock is only the fallback for a host that cannot say
/// when a process started, where the comparison cannot happen anyway.
fn taken_at(pid: Pid, probe: &dyn OwnerProbe, clock: &dyn Clock) -> DateTime<Utc> {
    probe.started(pid).unwrap_or_else(|| clock.now())
}

/// Whether the process a record names is still the process that wrote
/// the record.
///
/// A live pid is not enough: pids are reused, so a process that started
/// *after* the record was written is a different process wearing the
/// same number. Every reader of a pid somebody else wrote down asks
/// here — the lock, and `yunta cancel` before it signals anything.
pub fn holder_state(owner: &LockOwner, probe: &dyn OwnerProbe) -> Liveness {
    match probe.liveness(owner.pid) {
        Liveness::Alive => match probe.started(owner.pid) {
            Some(started) if started > owner.started_at => Liveness::Dead,
            _ => Liveness::Alive,
        },
        other => other,
    }
}

#[cfg(test)]
mod taken_at_tests {
    use super::*;

    /// A probe that says a process started at a fixed instant.
    struct Table(Option<DateTime<Utc>>);

    impl OwnerProbe for Table {
        fn liveness(&self, _pid: Pid) -> Liveness {
            Liveness::Alive
        }
        fn started(&self, _pid: Pid) -> Option<DateTime<Utc>> {
            self.0
        }
    }

    struct Frozen(DateTime<Utc>);

    impl Clock for Frozen {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    fn at(year: i32) -> DateTime<Utc> {
        chrono::TimeZone::with_ymd_and_hms(&Utc, year, 1, 1, 0, 0, 0).unwrap()
    }

    /// The record states when the process started *by the same table*
    /// `holder_state` reads. A run's clock answers about the run's
    /// timeline: one frozen in the past would make every live holder
    /// read as a stranger, and every lock stealable while its owner
    /// works.
    #[test]
    fn a_lock_records_the_start_the_process_table_reports_not_the_clock() {
        let pid = Pid::current();
        let table = Table(Some(at(2020)));
        let owner = LockOwner {
            pid,
            started_at: taken_at(pid, &table, &Frozen(at(2000))),
        };
        assert_eq!(owner.started_at, at(2020));
        assert_eq!(
            holder_state(&owner, &table),
            Liveness::Alive,
            "the process that took the lock still holds it"
        );
    }

    /// A host that cannot say when a process started leaves the clock as
    /// the only answer — and the comparison it feeds cannot happen
    /// either, so a live pid is taken at its word.
    #[test]
    fn a_host_with_no_process_table_falls_back_to_the_clock() {
        let pid = Pid::current();
        let blind = Table(None);
        let owner = LockOwner {
            pid,
            started_at: taken_at(pid, &blind, &Frozen(at(2000))),
        };
        assert_eq!(owner.started_at, at(2000));
        assert_eq!(holder_state(&owner, &blind), Liveness::Alive);
    }

    /// A pid the host handed to something that started later is a
    /// stranger, whatever the number says.
    #[test]
    fn a_pid_reused_by_a_later_process_reads_as_gone() {
        let pid = Pid::current();
        let owner = LockOwner {
            pid,
            started_at: at(2020),
        };
        assert_eq!(holder_state(&owner, &Table(Some(at(2024)))), Liveness::Dead);
    }
}
