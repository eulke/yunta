//! `yunta_adapters::process_start`: a process's start time from the
//! host's process table, or `None` where the host cannot tell.

use std::time::SystemTime;

use yunta_adapters::process_start::process_start;
use yunta_core::Pid;

#[cfg(target_os = "linux")]
#[test]
fn this_process_started_before_now_and_after_the_boot() {
    let started = process_start(Pid::current()).expect("linux publishes /proc/<pid>/stat");
    assert!(started <= SystemTime::now());
    let uptime = std::fs::read_to_string("/proc/uptime").unwrap();
    let seconds: f64 = uptime.split_whitespace().next().unwrap().parse().unwrap();
    let boot = SystemTime::now() - std::time::Duration::from_secs_f64(seconds);
    assert!(
        started >= boot - std::time::Duration::from_secs(2),
        "a process never starts before the boot"
    );
}

#[test]
fn a_pid_nothing_runs_under_has_no_start_time() {
    // The largest pid the kernel can hand out is far below this on any
    // default configuration.
    assert_eq!(process_start(Pid::try_from(i32::MAX).unwrap()), None);
}
