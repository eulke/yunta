//! `yunta_adapters::signal`: the one way a signal leaves the workspace,
//! answering with the kernel's own `errno` instead of a `kill` binary's
//! exit status.

use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

use yunta_adapters::signal::{liveness, signal_group, signal_process, Liveness, Signal, Target};
use yunta_core::Pid;

/// A blocker leading its own process group, so a group signal reaches
/// exactly it.
fn group_leader() -> Child {
    Command::new("tail")
        .args(["-f", "/dev/null"])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the blocker spawns")
}

fn pid_of(child: &Child) -> Pid {
    Pid::try_from(child.id()).expect("a spawned child has a positive pid")
}

#[test]
fn a_group_signal_reaches_the_leader_and_a_gone_group_is_success() {
    let mut child = group_leader();
    let pgid = pid_of(&child);
    assert_eq!(liveness(pgid), Liveness::Alive);

    signal_group(pgid, Signal::SIGKILL).expect("the group exists");
    let status = child.wait().expect("the leader is reaped");
    assert!(!status.success(), "SIGKILL ends the leader");

    assert_eq!(liveness(pgid), Liveness::Dead);
    signal_group(pgid, Signal::SIGKILL)
        .expect("a group that is already gone is the state a kill wants");
}

#[test]
fn a_signal_to_a_gone_process_names_the_target_and_the_errno() {
    let mut child = group_leader();
    let pid = pid_of(&child);
    signal_process(pid, Signal::SIGKILL).expect("the process exists");
    child.wait().expect("the child is reaped");

    let error = signal_process(pid, Signal::SIGTERM).expect_err("no process has that pid any more");
    assert_eq!(error.target, Target::Process(pid));
    assert_eq!(error.signal, Signal::SIGTERM);
    assert!(error.is_gone(), "ESRCH is the errno for a gone process");
    assert_eq!(
        error.to_string(),
        format!("failed to send SIGTERM to process {pid}")
    );
}

#[test]
fn liveness_is_alive_for_this_process_and_dead_for_a_reaped_child() {
    assert_eq!(liveness(Pid::current()), Liveness::Alive);

    let mut child = group_leader();
    let pid = pid_of(&child);
    signal_process(pid, Signal::SIGKILL).expect("the child exists");
    child.wait().expect("the child is reaped");
    assert_eq!(liveness(pid), Liveness::Dead);
}
