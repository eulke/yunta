//! The process groups under a group: what a session's CLI made for its tool
//! commands, which a signal to the CLI's own group never reaches.
//!
//! Both real CLIs start each tool command in a group of its own, under a
//! shell under the CLI, so the walk follows parent links down from every
//! member of the group and names each other group it meets. It reads the
//! host's process table once: `/proc` on Linux, `libproc` on macOS. It runs
//! while the group's processes live, because a process whose parent died
//! belongs to pid 1 and no parent link leads to it any more; what the mark
//! of `mark` reaches is that case.
//!
//! The walk never names the caller's own group, nor a group id of 1 or less:
//! a child can join the caller's group, and signalling either would reach far
//! more than a session's tree.

use std::collections::{BTreeMap, BTreeSet};
use std::io;

use thiserror::Error;

use crate::process::group::{force_kill_group, GroupError};
use crate::Pid;

/// One process as the host's table lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Process {
    pub(crate) pid: i32,
    pub(crate) parent: i32,
    pub(crate) group: i32,
}

/// Why the groups under a group could not all be closed.
#[derive(Debug, Error)]
pub enum TreeError {
    #[error("could not read the process table to walk the groups under {group}: {source}")]
    Read {
        group: Pid,
        #[source]
        source: io::Error,
    },
    #[error("process groups under {group} would not close: {}", listed(.closing))]
    NotClosed {
        group: Pid,
        closing: Vec<(Pid, GroupError)>,
    },
}

impl TreeError {
    /// The groups that may still run: each one that would not close, or none
    /// known when the table could not be read.
    pub fn groups(&self) -> Vec<Pid> {
        match self {
            TreeError::Read { .. } => Vec::new(),
            TreeError::NotClosed { closing, .. } => {
                closing.iter().map(|(group, _)| *group).collect()
            }
        }
    }
}

fn listed(closing: &[(Pid, GroupError)]) -> String {
    closing
        .iter()
        .map(|(group, error)| format!("{group} ({error})"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every process group under `group` other than `group` itself, found by
/// following parent links down from each of its members.
pub fn groups_under(group: Pid) -> io::Result<Vec<Pid>> {
    let own = rustix::process::getpgrp().as_raw_pid();
    Ok(under(&table()?, group.as_i32(), own)
        .into_iter()
        .filter_map(|group| Pid::try_from(group as u32).ok())
        .collect())
}

/// Closes every group under `group`, each until none of its members can
/// execute, and names each one that would not close.
pub async fn close_groups_under(group: Pid) -> Result<(), TreeError> {
    let groups = groups_under(group).map_err(|source| TreeError::Read { group, source })?;
    let mut closing = Vec::new();
    for under in groups {
        if let Err(error) = force_kill_group(under).await {
            closing.push((under, error));
        }
    }
    if closing.is_empty() {
        Ok(())
    } else {
        Err(TreeError::NotClosed { group, closing })
    }
}

/// One pass with SIGKILL at every group under `group`, with no wait: what
/// the drop of a session nobody ended can do. Best-effort by design.
pub fn signal_groups_under_now(group: Pid) {
    for under in groups_under(group).unwrap_or_default() {
        // best-effort: a group already gone is what the drop wanted.
        let _ =
            crate::process::signal::signal_group(under, crate::process::signal::Signal::SIGKILL);
    }
}

/// The groups under `group` in `table`, in order, never `group`, `own` or a
/// group id of 1 or less.
pub(crate) fn under(table: &[Process], group: i32, own: i32) -> Vec<i32> {
    let mut children: BTreeMap<i32, Vec<&Process>> = BTreeMap::new();
    for process in table {
        children.entry(process.parent).or_default().push(process);
    }
    let mut frontier: Vec<i32> = table
        .iter()
        .filter(|process| process.group == group)
        .map(|process| process.pid)
        .collect();
    let mut seen: BTreeSet<i32> = frontier.iter().copied().collect();
    let mut groups = BTreeSet::new();
    while let Some(parent) = frontier.pop() {
        for child in children.get(&parent).into_iter().flatten() {
            if !seen.insert(child.pid) {
                continue;
            }
            frontier.push(child.pid);
            if child.group != group && child.group != own && child.group > 1 {
                groups.insert(child.group);
            }
        }
    }
    groups.into_iter().collect()
}

/// Every process the host lists, with its parent and its group.
#[cfg(target_os = "linux")]
fn table() -> io::Result<Vec<Process>> {
    use procfs::process::all_processes;
    use procfs::ProcError;

    let mut table = Vec::new();
    for process in all_processes().map_err(io::Error::other)? {
        let stat = match process.and_then(|process| process.stat()) {
            Ok(stat) => stat,
            // A process that left between the listing and its read.
            Err(ProcError::NotFound(_)) => continue,
            Err(error) => return Err(io::Error::other(error)),
        };
        table.push(Process {
            pid: stat.pid,
            parent: stat.ppid,
            group: stat.pgrp,
        });
    }
    Ok(table)
}

/// Every process the host lists that this user may read, with its parent
/// and its group. Another user's process answers `EPERM` and is left out: it
/// is no session's.
#[cfg(target_os = "macos")]
fn table() -> io::Result<Vec<Process>> {
    use errno::{set_errno, Errno};
    use libproc::bsd_info::BSDInfo;
    use libproc::proc_pid::pidinfo;
    use libproc::processes::{pids_by_type, ProcFilter};

    // libproc treats a zero-byte result as an error when errno is nonzero, so
    // a value left by an earlier call is cleared first.
    set_errno(Errno(0));
    let pids = pids_by_type(ProcFilter::All)?;
    Ok(pids
        .into_iter()
        .filter_map(|pid| pidinfo::<BSDInfo>(pid as i32, 0).ok())
        .map(|info| Process {
            pid: info.pbi_pid as i32,
            parent: info.pbi_ppid as i32,
            group: info.pbi_pgid as i32,
        })
        .collect())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn table() -> io::Result<Vec<Process>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the process table is only read on Linux and macOS",
    ))
}

#[cfg(test)]
mod tests;
