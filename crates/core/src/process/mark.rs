//! A session's mark, and the sweep that closes what carries it.
//!
//! Every process a session's CLI starts inherits its environment, and both
//! real CLIs pass it on to their tool commands, so the mark set on the CLI is
//! in every process of the session's tree, including one whose parent left
//! and that now belongs to pid 1, where no parent link of [`tree`](super::tree)
//! leads. The sweep reads the environments of this user's processes and
//! closes each one holding the mark. A session's sweep reads only those that
//! started no earlier than a minute before its CLI, since none older can have
//! inherited the mark; a run's, from `yunta cancel` or `yunta close`, knows no
//! start and reads them all.
//!
//! The mark names its run and is unique to its session, so a session's sweep
//! closes only its own, and `yunta cancel` and `yunta close` close what a dead
//! engine's sessions left from the run's id alone. It is not a secret: it
//! opens nothing.
//!
//! What it cannot reach: a process that cleared its environment, and, on
//! macOS, one running a platform binary (`/bin/sleep`, `sh`, `zsh`, `perl`,
//! `ruby`) where macOS hides its environment from every other process: a Mac
//! with System Integrity Protection on was measured hiding it, and GitHub's
//! macOS runner showing it. Such a process reads as unmarked. Linux shows every process of the user through
//! `/proc`; macOS is read through `sysinfo`, because `KERN_PROCARGS2` needs
//! `unsafe`.

use std::io;
use std::time::{Duration, SystemTime};

use thiserror::Error;

use crate::process::process_start::process_start;
use crate::process::signal::{signal_process, Signal};
use crate::{Pid, RunId};

/// The variable a session's CLI carries its mark in. Its name holds none of
/// `KEY`, `SECRET` or `TOKEN`, which Codex can filter from what its commands
/// inherit.
pub const MARK_VAR: &str = "YUNTA_SESSION_MARK";

/// How long a pass waits before looking again for what it closed.
const PASS_INTERVAL: Duration = Duration::from_millis(10);

/// How many passes a sweep makes before it names what would not die: a
/// process that forks while it is closed shows up on the next pass, and fifty
/// passes reach far deeper than a session's tree, within half a second.
const MOST_PASSES: usize = 50;

/// How long before a session's CLI a process may have started and still be
/// read for its mark. None can have inherited it, but a wall clock stepped
/// back between the CLI's start and a command's places the command before
/// the CLI, and a minute covers the steps a synchronised clock takes.
const CLOCK_STEP: Duration = Duration::from_secs(60);

/// One session's mark: `<run id>.<random v4 id>`. Not a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMark(String);

impl SessionMark {
    /// A new mark for a session of `run`, unlike any other.
    pub fn new(run: &RunId) -> Self {
        Self(format!("{run}.{}", uuid::Uuid::new_v4().simple()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// This session's mark alone, whose CLI started at `since`.
    pub fn marks(&self, since: SystemTime) -> Marks {
        Marks::Session {
            mark: self.clone(),
            since,
        }
    }

    /// The marks of every session of `run`.
    pub fn of_run(run: &RunId) -> Marks {
        Marks::Run(run.clone())
    }
}

/// Which marks a sweep closes: one session's, or every session's of a run.
/// A session's knows when its CLI started (`since`), and no process that
/// started long before it can have inherited the mark; a run's knows no
/// start, since a dead engine's sessions left none behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Marks {
    Session {
        mark: SessionMark,
        since: SystemTime,
    },
    Run(RunId),
}

impl Marks {
    /// The earliest start a process carrying one of these can have, when a
    /// start is known: a minute before the session's CLI.
    fn earliest(&self) -> Option<SystemTime> {
        match self {
            Marks::Session { since, .. } => Some(
                since
                    .checked_sub(CLOCK_STEP)
                    .unwrap_or(SystemTime::UNIX_EPOCH),
            ),
            Marks::Run(_) => None,
        }
    }

    /// Whether `value`, read from a process's environment, is one of these.
    pub fn hold(&self, value: &str) -> bool {
        match self {
            Marks::Session { mark, .. } => value == mark.as_str(),
            Marks::Run(run) => value
                .strip_prefix(run.as_str())
                .is_some_and(|rest| rest.starts_with('.')),
        }
    }
}

/// A process found carrying a mark, with the start time it was found with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    pub pid: Pid,
    pub start: Option<SystemTime>,
}

/// Why a sweep did not close everything it found.
#[derive(Debug, Error)]
pub enum MarkError {
    #[error("could not read the processes' environments: {0}")]
    Read(#[source] io::Error),
    #[error("processes carrying a session's mark would not die: {}", listed(.0))]
    NotClosed(Vec<Pid>),
}

impl MarkError {
    /// What may still run: each process that would not die.
    pub fn pids(&self) -> Vec<Pid> {
        match self {
            MarkError::Read(_) => Vec::new(),
            MarkError::NotClosed(pids) => pids.clone(),
        }
    }
}

fn listed(pids: &[Pid]) -> String {
    pids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every process of this user whose environment holds one of `marks`, each
/// with the start time it has now. A process whose environment the host
/// hides reads as unmarked, and one that started before `marks` can have
/// been inherited is not read at all.
pub fn marked(marks: &Marks) -> io::Result<Vec<Found>> {
    let earliest = marks.earliest();
    Ok(environments(earliest)?
        .into_iter()
        .filter(|(_, value)| marks.hold(value))
        .map(|(pid, _)| Found {
            pid,
            start: process_start(pid),
        })
        .filter(|found| {
            earliest.is_none_or(|earliest| found.start.is_some_and(|start| start >= earliest))
        })
        .collect())
}

/// Closes every process carrying one of `marks`, pass after pass until a
/// pass finds none, and names what would not die.
pub async fn close_marked(marks: &Marks) -> Result<(), MarkError> {
    let own = std::process::id() as i32;
    let mut pass = tokio::time::interval(PASS_INTERVAL);
    let mut left = Vec::new();
    for _ in 0..MOST_PASSES {
        pass.tick().await;
        let found = marked(marks).map_err(MarkError::Read)?;
        left = to_signal(&found, process_start, own);
        if left.is_empty() {
            return Ok(());
        }
        for pid in &left {
            // best-effort: one that left first is what was wanted; the next pass looks again.
            let _ = signal_process(*pid, Signal::SIGKILL);
        }
    }
    Err(MarkError::NotClosed(left))
}

/// One pass with SIGKILL at what carries one of `marks`, with no wait: what
/// the drop of a session nobody ended can do. Best-effort by design.
pub fn signal_marked_now(marks: &Marks) {
    let found = marked(marks).unwrap_or_default();
    for pid in to_signal(&found, process_start, std::process::id() as i32) {
        // best-effort: one that left first is what the drop wanted.
        let _ = signal_process(pid, Signal::SIGKILL);
    }
}

/// The pids of `found` that may be signalled: never the caller, never a pid
/// of 1 or less, and only a process whose start time is still the one it was
/// found with, so a pid the host gave to another process is never signalled.
pub(crate) fn to_signal(
    found: &[Found],
    start_now: impl Fn(Pid) -> Option<SystemTime>,
    own: i32,
) -> Vec<Pid> {
    found
        .iter()
        .filter(|process| process.pid.as_i32() > 1 && process.pid.as_i32() != own)
        .filter(|process| process.start.is_some() && start_now(process.pid) == process.start)
        .map(|process| process.pid)
        .collect()
}

/// The mark each process of this user carries, if any. `/proc` answers
/// each environment cheaply, so every process is read, whatever `_earliest`.
#[cfg(target_os = "linux")]
fn environments(_earliest: Option<SystemTime>) -> io::Result<Vec<(Pid, String)>> {
    use procfs::process::all_processes;

    let own = crate::process::user::uid();
    let mut carried = Vec::new();
    for process in all_processes().map_err(io::Error::other)?.flatten() {
        // A process that left, or whose environment cannot be read, carries
        // nothing this sweep can see.
        if process.uid().ok() != Some(own) {
            continue;
        }
        let Ok(environment) = process.environ() else {
            continue;
        };
        let value = environment
            .get(std::ffi::OsStr::new(MARK_VAR))
            .and_then(|value| value.to_str());
        if let (Some(value), Ok(pid)) = (value, Pid::try_from(process.pid as u32)) {
            carried.push((pid, value.to_string()));
        }
    }
    Ok(carried)
}

/// The mark each process of this user carries, if any, read only from the
/// processes that started no earlier than `earliest`. A listing through
/// `sysinfo` reads far more of each process than a start, so the candidates
/// are listed as the walk lists the table, and `sysinfo` reads only their
/// environments. A platform binary whose environment macOS hides reads as
/// unmarked.
#[cfg(target_os = "macos")]
fn environments(earliest: Option<SystemTime>) -> io::Result<Vec<(Pid, String)>> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

    let candidates = candidates(earliest)?;
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let mut system = System::new().map_err(|error| io::Error::other(error.to_string()))?;
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&candidates),
        true,
        ProcessRefreshKind::nothing().with_environ(UpdateKind::Always),
    );
    let prefix = format!("{MARK_VAR}=");
    Ok(candidates
        .iter()
        .filter_map(|pid| {
            let value = system
                .process(*pid)?
                .environ()
                .iter()
                .find_map(|entry| entry.to_str()?.strip_prefix(&prefix))?;
            Some((Pid::try_from(pid.as_u32()).ok()?, value.to_string()))
        })
        .collect())
}

/// This user's processes that started no earlier than `earliest`. Another
/// user's process answers `EPERM` and is left out: it is no session's.
#[cfg(target_os = "macos")]
fn candidates(earliest: Option<SystemTime>) -> io::Result<Vec<sysinfo::Pid>> {
    use errno::{set_errno, Errno};
    use libproc::bsd_info::BSDInfo;
    use libproc::proc_pid::pidinfo;
    use libproc::processes::{pids_by_type, ProcFilter};

    let own = crate::process::user::uid();
    // libproc treats a zero-byte result as an error when errno is nonzero, so
    // a value left by an earlier call is cleared first.
    set_errno(Errno(0));
    Ok(pids_by_type(ProcFilter::All)?
        .into_iter()
        .filter_map(|pid| pidinfo::<BSDInfo>(pid as i32, 0).ok())
        .filter(|info| info.pbi_uid == own)
        .filter(|info| {
            let started = SystemTime::UNIX_EPOCH
                + Duration::from_secs(info.pbi_start_tvsec)
                + Duration::from_micros(info.pbi_start_tvusec);
            earliest.is_none_or(|earliest| started >= earliest)
        })
        .map(|info| sysinfo::Pid::from_u32(info.pbi_pid))
        .collect())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn environments(_earliest: Option<SystemTime>) -> io::Result<Vec<(Pid, String)>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "another process's environment is only read on Linux and macOS",
    ))
}

#[cfg(test)]
mod tests;
