//! `yunta_core::process::process_start`: a process's start time from the
//! host's process table, or `None` where the host cannot tell.

use std::time::{Duration, SystemTime};

use yunta_core::process::process_start::process_start;
use yunta_core::Pid;

/// A start read from the wrong place would be far from either side. Linux
/// rounds to its boot time's whole second, hence the margin before the spawn.
#[test]
fn a_process_started_now_reports_a_start_between_just_before_and_now() {
    let before = SystemTime::now();
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let started = process_start(Pid::try_from(child.id()).unwrap());
    let now = SystemTime::now();
    child.kill().unwrap();
    child.wait().unwrap();

    let started = started.expect("a host with a process table reports this user's process");
    assert!(
        started >= before - Duration::from_secs(2),
        "started {started:?}, spawned after {before:?}"
    );
    assert!(started <= now, "started {started:?}, read at {now:?}");
}

#[cfg(target_os = "linux")]
#[test]
fn this_process_started_before_now_and_after_the_boot() {
    let started = process_start(Pid::current()).expect("linux publishes /proc/<pid>/stat");
    assert!(started <= SystemTime::now());
    let uptime = std::fs::read_to_string("/proc/uptime").unwrap();
    let seconds: f64 = uptime.split_whitespace().next().unwrap().parse().unwrap();
    let boot = SystemTime::now() - Duration::from_secs_f64(seconds);
    assert!(
        started >= boot - Duration::from_secs(2),
        "a process never starts before the boot"
    );
}

#[test]
fn a_pid_nothing_runs_under_has_no_start_time() {
    // The largest pid the kernel can hand out is far below this on any
    // default configuration.
    assert_eq!(process_start(Pid::try_from(i32::MAX).unwrap()), None);
}

/// Pid 1 runs as root: Linux publishes its start to anyone.
#[cfg(target_os = "linux")]
#[test]
fn another_users_process_has_a_start_time_on_linux() {
    assert!(process_start(Pid::try_from(1_i32).unwrap()).is_some());
}

/// Pid 1 runs as root: macOS answers only about this user's processes.
#[cfg(target_os = "macos")]
#[test]
fn another_users_process_has_no_start_time_on_macos() {
    assert_eq!(process_start(Pid::try_from(1_i32).unwrap()), None);
}
