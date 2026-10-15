//! A lock file owned by one process, honest about who holds it. The
//! file's content is the holder's [`LockOwner`]; taking the lock is one
//! atomic `create_new`; a holder that is gone is replaced by removing
//! its file and creating anew, never by writing over it — two
//! stealers racing both go through `create_new` again, and exactly one
//! wins. "Gone" is the probe's answer, and a holder whose state the
//! probe cannot tell counts as present: a run must never lose the
//! exclusivity it still holds without a sound. A gone holder whose work
//! still runs is kept as well: the caller's `keep` is asked before the
//! file is touched.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_core::durable::{self, Durability};
use yunta_core::process::signal::{self, Liveness};
use yunta_core::{Clock, Pid};

use crate::pace::Pace;

/// What the host says about a lock's holder. `Send + Sync` so a
/// `&dyn OwnerProbe` can be held across the awaits in [`acquire`] in a
/// `Send` async body — the in-process control plane prepares a worktree
/// from one.
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
    /// Version of this file's own schema.
    #[serde(default)]
    pub schema_version: u32,
    pub pid: Pid,
    pub started_at: DateTime<Utc>,
}

impl yunta_core::persisted::Persisted for LockOwner {
    /// The version this binary writes; a newer one is refused, never guessed at.
    const SCHEMA_VERSION: u32 = 1;
    const NAME: &'static str = "isolation lock";
    const ENCODING: yunta_core::persisted::Encoding = yunta_core::persisted::Encoding::Json;
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
    /// The lock was free; the record this process wrote is its holder.
    Fresh(LockOwner),
    /// The previous holder was gone: `owner` is the record this process
    /// wrote, and `dead` the record it replaced, for the report.
    Stolen { owner: LockOwner, dead: LockOwner },
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
    /// The file exists but names no readable holder: it is corrupted, or a
    /// build that died between creating it and writing it left it empty.
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
    /// A dead holder whose agents still work: its lock is neither taken
    /// nor removed.
    #[error(
        "`{lock_path}` is kept: its holder pid {} is gone, but its agents still work",
        owner.pid
    )]
    Kept {
        lock_path: PathBuf,
        owner: LockOwner,
    },
    /// The cancellation token stopped the wait.
    #[error("the wait for `{lock_path}` was cancelled")]
    Cancelled { lock_path: PathBuf },
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
        schema_version: <LockOwner as yunta_core::persisted::Persisted>::SCHEMA_VERSION,
        pid,
        started_at: taken_at(pid, probe, clock),
    };
    let json = yunta_core::persisted::PersistedDoc::of(record)
        .write()
        .map_err(|e| io("encode the holder of", std::io::Error::other(e)))?;
    // blocking: `hand_over` names a file's owner before the process it
    // names exists, on the caller's own thread and outside any run.
    durable::replace(lock_path, &json, Durability::Atomic)
        .map_err(|source| io("write the holder of", source))
}

/// Takes the lock at `lock_path` for this process, recording it as the
/// holder with the clock's time. A gone holder (dead, or a live process
/// that started after the lock was taken and so merely reuses the pid)
/// is stolen, unless `keep` says the work it left is still going on. A
/// present one is refused or waited on per `contention`; a wait ends when
/// `cancel` fires.
pub async fn acquire(
    lock_path: &Path,
    contention: Contention,
    probe: &dyn OwnerProbe,
    clock: &dyn Clock,
    cancel: &CancellationToken,
    keep: &(dyn Fn(&LockOwner) -> bool + Sync),
) -> Result<Acquired, LockError> {
    let deadline = match contention {
        Contention::Refuse => None,
        Contention::Wait { patience, .. } => Some(Instant::now() + patience),
    };
    let mut stolen_from = None;
    loop {
        let record = this_process(probe, clock);
        if create_record(lock_path, &record)? {
            return Ok(match stolen_from {
                Some(dead) => Acquired::Stolen {
                    owner: record,
                    dead,
                },
                None => Acquired::Fresh(record),
            });
        }
        // Another holder created the lock first: the present-or-stale check takes over.
        let owner = read_holder(lock_path).await;
        let liveness = owner.as_ref().map(|owner| holder_state(owner, probe));
        if let (Some(owner), Some(Liveness::Dead)) = (&owner, liveness) {
            if keep(owner) {
                return Err(LockError::Kept {
                    lock_path: lock_path.to_path_buf(),
                    owner: owner.clone(),
                });
            }
            // Remove, then retry: the atomic `create_new` above
            // decides which of two concurrent stealers wins.
            remove_stale(lock_path).await?;
            stolen_from = Some(owner.clone());
            continue;
        }
        // Alive, unknown, or unreadable: present.
        match (deadline, contention) {
            (Some(deadline), Contention::Wait { poll, .. }) if Instant::now() < deadline => {
                pause(lock_path, poll, cancel).await?;
            }
            (Some(_), _) => {
                return Err(LockError::Timeout {
                    lock_path: lock_path.to_path_buf(),
                    owner,
                });
            }
            (None, _) => return Err(refusal(lock_path, owner, liveness)),
        }
    }
}

/// The record this process writes as a lock's holder: its pid, and when
/// it started.
fn this_process(probe: &dyn OwnerProbe, clock: &dyn Clock) -> LockOwner {
    let pid = Pid::current();
    LockOwner {
        schema_version: <LockOwner as yunta_core::persisted::Persisted>::SCHEMA_VERSION,
        pid,
        started_at: taken_at(pid, probe, clock),
    }
}

/// Creates the lock at `lock_path` holding `record`. The record is whole
/// before the lock's name exists, so no reader meets an empty lock. `false`
/// means another holder created the lock first.
fn create_record(lock_path: &Path, record: &LockOwner) -> Result<bool, LockError> {
    let json = yunta_core::persisted::PersistedDoc::of(record.clone())
        .write()
        .map_err(|e| io_error(lock_path, "encode the holder of", std::io::Error::other(e)))?;
    // blocking: taking a lock is `create_new` — the one operation
    // whose atomicity is the whole mechanism, and which `tokio::fs`
    // would run on a pool thread without making any more atomic.
    durable::create_new(lock_path, &json, Durability::Atomic)
        .map_err(|source| io_error(lock_path, "create", source))
}

/// The record a lock file names, when the file reads as one.
async fn read_holder(lock_path: &Path) -> Option<LockOwner> {
    tokio::fs::read(lock_path)
        .await
        .ok()
        .and_then(|bytes| yunta_core::persisted::PersistedDoc::<LockOwner>::read(&bytes).ok())
        .map(|owner| owner.doc)
}

/// Removes the file of a gone holder, so the next `create_new` takes the lock.
async fn remove_stale(lock_path: &Path) -> Result<(), LockError> {
    match tokio::fs::remove_file(lock_path).await {
        Err(source) if source.kind() != std::io::ErrorKind::NotFound => {
            Err(io_error(lock_path, "remove the stale", source))
        }
        _ => Ok(()),
    }
}

/// Waits one `poll` for the holder to let go; `cancel` ends the wait at once.
async fn pause(
    lock_path: &Path,
    poll: Duration,
    cancel: &CancellationToken,
) -> Result<(), LockError> {
    tokio::select! {
        () = Pace::every(poll).beat() => Ok(()),
        _ = cancel.cancelled() => Err(LockError::Cancelled {
            lock_path: lock_path.to_path_buf(),
        }),
    }
}

/// Why a present lock cannot be taken: the holder it names, when the file
/// names one that reads.
fn refusal(lock_path: &Path, owner: Option<LockOwner>, liveness: Option<Liveness>) -> LockError {
    match (owner, liveness) {
        (Some(owner), Some(liveness)) => LockError::Held {
            lock_path: lock_path.to_path_buf(),
            owner,
            liveness,
        },
        _ => LockError::Unreadable {
            lock_path: lock_path.to_path_buf(),
        },
    }
}

/// An operation on the lock file at `lock_path` that failed.
fn io_error(lock_path: &Path, action: &'static str, source: std::io::Error) -> LockError {
    LockError::Io {
        action,
        lock_path: lock_path.to_path_buf(),
        source,
    }
}

/// Removes the lock at `lock_path`, but only while it still names `owner`.
///
/// Safe because nobody takes the lock of a live owner: a file that names
/// `owner` is the lock this process took and still holds, and a file that
/// names anybody else has moved on, so it is left alone.
pub fn release(lock_path: &Path, owner: &LockOwner) -> Result<(), LockError> {
    // A release is one read and at most one removal, and it runs from a
    // `Drop`, which has no `await` to give.
    // blocking: one read on this thread, from the `Drop` that releases.
    let bytes = match std::fs::read(lock_path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error(lock_path, "remove", source)),
    };
    let names_owner = yunta_core::persisted::PersistedDoc::<LockOwner>::read(&bytes)
        .is_ok_and(|record| &record.doc == owner);
    if !names_owner {
        return Ok(());
    }
    // blocking: the same removal as the read above, from the same `Drop`.
    match std::fs::remove_file(lock_path) {
        Err(source) if source.kind() != std::io::ErrorKind::NotFound => {
            Err(io_error(lock_path, "remove", source))
        }
        _ => Ok(()),
    }
}

/// When the process a record names started, as the record has to state
/// it: read from the same process table [`holder_state`] compares it
/// against. Every record of a pid somebody else will read asks here —
/// the lock, and the run's process registry.
///
/// Not the clock's `now()`. The comparison that decides whether a pid
/// was reused reads the host's process table, and a run's clock answers
/// about the run's timeline — a frozen one, or one an hour behind the
/// host, would make every live holder read as a stranger and every lock
/// stealable. The clock is only the fallback for a host that cannot say
/// when a process started, where the comparison cannot happen anyway.
pub(crate) fn taken_at(pid: Pid, probe: &dyn OwnerProbe, clock: &dyn Clock) -> DateTime<Utc> {
    probe.started(pid).unwrap_or_else(|| clock.now())
}

/// Whether the process a record names is still the process that wrote
/// the record: dead when the probe says so, or when the live process with
/// that pid started after the lock was taken, a newcomer that got the
/// holder's pid, not the holder.
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
            schema_version: <LockOwner as yunta_core::persisted::Persisted>::SCHEMA_VERSION,
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
            schema_version: <LockOwner as yunta_core::persisted::Persisted>::SCHEMA_VERSION,
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
            schema_version: <LockOwner as yunta_core::persisted::Persisted>::SCHEMA_VERSION,
            pid,
            started_at: at(2020),
        };
        assert_eq!(holder_state(&owner, &Table(Some(at(2024)))), Liveness::Dead);
    }
}
