//! Closing a process group without assuming one group signal reaches a
//! stable snapshot of its members. A running process can fork while the
//! kernel delivers `SIGSTOP`; we stop until two consecutive observations
//! agree on the members, then kill and confirm none can still execute.

use std::io;
use std::time::Duration;

use thiserror::Error;

#[cfg(target_os = "macos")]
use crate::process::signal::{liveness, Liveness};
use crate::process::signal::{signal_group, Signal, SignalError};
use crate::Pid;

/// Observes a child after it exits without collecting it. Keeping the
/// zombie waitable until process-group cleanup is finished prevents its PID
/// from being reused as a different group's ID while signals are sent.
pub fn child_has_exited_unreaped(pid: Pid) -> io::Result<bool> {
    use rustix::process::{waitid, Pid as RustixPid, WaitId, WaitIdOptions};

    let pid = RustixPid::from_raw(pid.as_i32()).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "a child pid must be positive")
    })?;
    let status = waitid(
        WaitId::Pid(pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    )
    .map_err(io::Error::other)?;
    Ok(status.is_some_and(|status| status.exited() || status.killed() || status.dumped()))
}

const OBSERVATION_INTERVAL: Duration = Duration::from_millis(10);
const MAX_OBSERVATIONS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Running,
    Stopped,
    Terminated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Member {
    pid: i32,
    state: State,
}

#[derive(Debug, Error)]
pub enum GroupError {
    #[error("failed to signal {signal} to process group {pgid}: {source:?}")]
    Signal {
        pgid: Pid,
        signal: Signal,
        #[source]
        source: SignalError,
    },
    #[error("failed to inspect process group {pgid}: {source}")]
    Inspect {
        pgid: Pid,
        #[source]
        source: io::Error,
    },
    #[error("process group {pgid} did not reach a stable stopped state: {members}")]
    DidNotStop { pgid: Pid, members: String },
    #[error("process group {pgid} still has executable members after SIGKILL: {members}")]
    DidNotExit { pgid: Pid, members: String },
    #[error("{cause}; emergency SIGKILL for process group {pgid} also failed: {signal:?}")]
    EmergencyKill {
        pgid: Pid,
        #[source]
        cause: Box<GroupError>,
        signal: SignalError,
    },
}

/// Stops then kills every member of `pgid`, including descendants that
/// joined while the first stop signal was being delivered. The caller must
/// keep the group leader unreaped until this returns: its pid is the group
/// id, and retaining it prevents that identity from being recycled while
/// signals are still sent.
pub async fn force_kill_group(pgid: Pid) -> Result<(), GroupError> {
    let last_members = match stop_until_stable(pgid).await {
        Ok(members) => members,
        Err(error) => return Err(emergency_kill(pgid, error)),
    };

    // An already-empty group, or one represented only by zombies, needs
    // no signal. In particular, macOS can answer EPERM when killpg targets
    // a group whose only remaining member has already exited.
    if last_members
        .iter()
        .all(|member| member.state == State::Terminated)
    {
        return Ok(());
    }

    if let Err(error) = signal_group(pgid, Signal::SIGKILL) {
        let cause = GroupError::Signal {
            pgid,
            signal: Signal::SIGKILL,
            source: error,
        };
        return Err(match signal_group(pgid, Signal::SIGCONT) {
            Ok(()) => cause,
            Err(signal) => GroupError::EmergencyKill {
                pgid,
                cause: Box::new(cause),
                signal,
            },
        });
    }

    match confirm_group_exit(pgid).await {
        Ok(()) => Ok(()),
        Err(error) => Err(emergency_kill(pgid, error)),
    }
}

async fn stop_until_stable(pgid: Pid) -> Result<Vec<Member>, GroupError> {
    let mut previous_stopped_members: Option<Vec<i32>> = None;
    let mut last_members = Vec::new();
    let mut observation = tokio::time::interval(OBSERVATION_INTERVAL);
    observation.tick().await;
    for _ in 0..MAX_OBSERVATIONS {
        let members = match inspect(pgid) {
            Ok(members) => members,
            Err(source) => return Err(GroupError::Inspect { pgid, source }),
        };
        last_members = members.clone();
        let stopped = members.iter().all(|member| member.state != State::Running);
        if stopped {
            let pids: Vec<_> = members.iter().map(|member| member.pid).collect();
            if previous_stopped_members.as_ref() == Some(&pids) {
                return Ok(last_members);
            }
            previous_stopped_members = Some(pids);
        } else {
            // A child may have been created after the kernel's first walk
            // through the group. Stop again, then require a stable view.
            send(pgid, Signal::SIGSTOP)?;
            previous_stopped_members = None;
        }
        observation.tick().await;
    }

    Err(GroupError::DidNotStop {
        pgid,
        members: format!("{last_members:?}"),
    })
}

async fn confirm_group_exit(pgid: Pid) -> Result<(), GroupError> {
    let mut observation = tokio::time::interval(OBSERVATION_INTERVAL);
    observation.tick().await;
    let mut last_members = Vec::new();
    for _ in 0..MAX_OBSERVATIONS {
        let members = match inspect(pgid) {
            Ok(members) => members,
            Err(source) => return Err(GroupError::Inspect { pgid, source }),
        };
        if members
            .iter()
            .all(|member| member.state == State::Terminated)
        {
            return Ok(());
        }
        last_members = members;
        observation.tick().await;
    }

    Err(GroupError::DidNotExit {
        pgid,
        members: format!("{last_members:?}"),
    })
}

fn send(pgid: Pid, signal: Signal) -> Result<(), GroupError> {
    signal_group(pgid, signal).map_err(|source| GroupError::Signal {
        pgid,
        signal,
        source,
    })
}

fn emergency_kill(pgid: Pid, cause: GroupError) -> GroupError {
    match signal_group(pgid, Signal::SIGKILL) {
        Ok(()) => cause,
        Err(signal) => GroupError::EmergencyKill {
            pgid,
            cause: Box::new(cause),
            signal,
        },
    }
}

#[cfg(target_os = "linux")]
fn inspect(pgid: Pid) -> io::Result<Vec<Member>> {
    use procfs::process::{all_processes, ProcState};
    use procfs::ProcError;

    let processes = all_processes().map_err(proc_error)?;
    let mut members = Vec::new();
    for process in processes {
        let process = match process {
            Ok(process) => process,
            Err(ProcError::NotFound(_)) => continue,
            Err(error) => return Err(proc_error(error)),
        };
        let stat = match process.stat() {
            Ok(stat) => stat,
            Err(ProcError::NotFound(_)) => continue,
            Err(error) => return Err(proc_error(error)),
        };
        if stat.pgrp != pgid.as_i32() {
            continue;
        }
        let state = match stat.state().map_err(proc_error)? {
            ProcState::Stopped | ProcState::Tracing => State::Stopped,
            ProcState::Zombie | ProcState::Dead => State::Terminated,
            ProcState::Running
            | ProcState::Sleeping
            | ProcState::Waiting
            | ProcState::Wakekill
            | ProcState::Waking
            | ProcState::Parked
            | ProcState::Idle => State::Running,
        };
        members.push(Member {
            pid: stat.pid,
            state,
        });
    }
    members.sort_by_key(|member| member.pid);
    Ok(members)
}

#[cfg(target_os = "linux")]
fn proc_error(error: procfs::ProcError) -> io::Error {
    io::Error::other(error)
}

#[cfg(target_os = "macos")]
fn inspect(pgid: Pid) -> io::Result<Vec<Member>> {
    use libproc::bsd_info::BSDInfo;
    use libproc::proc_pid::pidinfo;

    let pids = pids_in_group(pgid)?;
    let mut members = Vec::new();
    for pid in pids {
        let info = match pidinfo::<BSDInfo>(pid as i32, 0) {
            Ok(info) => info,
            Err(error) => {
                let process = Pid::try_from(pid).ok();
                if process.is_some_and(|pid| liveness(pid) == Liveness::Dead) {
                    continue;
                }
                // libproc can list a process immediately before it becomes
                // a zombie and then fail `proc_pidinfo` with ESRCH. It no
                // longer executes; keep that fact in the snapshot so the
                // next observation still has to confirm stable membership.
                if error.contains("No such process") {
                    members.push(Member {
                        pid: pid as i32,
                        state: State::Terminated,
                    });
                    continue;
                }
                return Err(io::Error::other(format!(
                    "libproc could not inspect process {pid}: {error}"
                )));
            }
        };
        if info.pbi_pgid != pgid.as_i32() as u32 {
            continue;
        }
        let state = match info.pbi_status {
            libc::SSTOP => State::Stopped,
            libc::SZOMB => State::Terminated,
            _ => State::Running,
        };
        members.push(Member {
            pid: info.pbi_pid as i32,
            state,
        });
    }
    members.sort_by_key(|member| member.pid);
    Ok(members)
}

#[cfg(target_os = "macos")]
fn pids_in_group(pgid: Pid) -> io::Result<Vec<u32>> {
    use errno::{set_errno, Errno};
    use libproc::processes::{pids_by_type, ProcFilter};

    // libproc treats a zero-byte result as an error when errno is nonzero.
    // proc_listpids can legitimately return zero for an empty group without
    // clearing the thread-local errno left by an unrelated earlier call.
    set_errno(Errno(0));
    let pids = pids_by_type(ProcFilter::ByProgramGroup {
        pgrpid: pgid.as_i32() as u32,
    })?;
    Ok(pids)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn inspect(_pgid: Pid) -> io::Result<Vec<Member>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "process-group inspection is only implemented for Linux and macOS",
    ))
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    use super::*;
    use crate::process::signal::{liveness, Liveness};

    #[tokio::test]
    async fn force_kill_group_stops_and_kills_a_live_group() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 30 & wait"])
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pgid = Pid::try_from(child.id()).unwrap();

        force_kill_group(pgid).await.unwrap();
        assert!(inspect(pgid)
            .unwrap()
            .iter()
            .all(|member| member.state == State::Terminated));
        child.wait().unwrap();
    }

    #[tokio::test]
    async fn exit_observation_keeps_the_leader_waitable_until_group_cleanup() {
        let mut child = Command::new("sh")
            .args(["-c", "exit 0"])
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pgid = Pid::try_from(child.id()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if child_has_exited_unreaped(pgid).unwrap() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the child exits");

        assert!(child_has_exited_unreaped(pgid).unwrap());
        assert_eq!(
            liveness(pgid),
            Liveness::Alive,
            "WNOWAIT leaves the zombie waitable"
        );
        child.wait().unwrap();
        assert_eq!(
            liveness(pgid),
            Liveness::Dead,
            "the ordinary wait reaps the leader"
        );
    }

    #[tokio::test]
    async fn an_empty_group_needs_no_signal() {
        let pgid = Pid::try_from(999_999_999_u32).unwrap();
        force_kill_group(pgid).await.unwrap();
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn an_empty_group_ignores_errno_left_by_an_unrelated_call() {
        errno::set_errno(errno::Errno(316));
        let pgid = Pid::try_from(999_999_999_u32).unwrap();
        force_kill_group(pgid).await.unwrap();
    }

    #[tokio::test]
    async fn zombies_are_not_running_group_members() {
        let mut child = Command::new("sh")
            .args(["-c", "exit 0"])
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pgid = Pid::try_from(child.id()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !child_has_exited_unreaped(pgid).unwrap() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the child exits");

        assert!(inspect(pgid)
            .unwrap()
            .iter()
            .all(|member| member.state == State::Terminated));
        assert_eq!(
            liveness(pgid),
            Liveness::Alive,
            "the child remains waitable"
        );
        child.wait().unwrap();
    }
}
