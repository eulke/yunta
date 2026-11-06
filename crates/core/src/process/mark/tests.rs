use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;

fn run(id: &str) -> RunId {
    RunId::from(id)
}

fn at(seconds: u64) -> Option<SystemTime> {
    Some(UNIX_EPOCH + Duration::from_secs(seconds))
}

fn found(pid: u32, start: Option<SystemTime>) -> Found {
    Found {
        pid: Pid::try_from(pid).unwrap(),
        start,
    }
}

/// Two marks of one session's run differ, and both name that run.
#[test]
fn every_mark_names_its_run_and_is_its_own() {
    let this = run("01KMARKRUN0000000000000001");
    let first = SessionMark::new(&this);
    let second = SessionMark::new(&this);

    assert_ne!(first, second);
    assert!(first.as_str().starts_with("01KMARKRUN0000000000000001."));
    assert!(SessionMark::of_run(&this).hold(first.as_str()));
    assert!(SessionMark::of_run(&this).hold(second.as_str()));
}

/// A session's marks hold its own value only, and a run's marks hold no
/// value of a run whose id only starts like it.
#[test]
fn marks_hold_their_own_values_and_no_others() {
    let this = run("01KMARKRUN0000000000000001");
    let mark = SessionMark::new(&this);
    let other = SessionMark::new(&this);
    let longer = format!("{}X.{}", this.as_str(), "0".repeat(32));

    let marks = mark.marks(std::time::SystemTime::UNIX_EPOCH);
    assert!(marks.hold(mark.as_str()));
    assert!(!marks.hold(other.as_str()));
    assert!(!SessionMark::of_run(&this).hold(&longer));
    assert!(!SessionMark::of_run(&this).hold(this.as_str()));
}

/// Only a process that still has the start time it was found with is
/// signalled: a pid the host gave to another process since is left alone.
#[test]
fn a_pid_whose_start_time_changed_is_not_signalled() {
    let found = [found(500, at(10)), found(501, at(20))];
    let now = |pid: Pid| if pid.as_u32() == 500 { at(10) } else { at(99) };

    assert_eq!(to_signal(&found, now, 1), vec![Pid::try_from(500).unwrap()]);
}

/// The caller is never signalled, nor a process whose start time could not
/// be read.
#[test]
fn the_caller_and_an_unread_start_are_never_signalled() {
    let found = [found(600, at(10)), found(601, None)];

    assert_eq!(to_signal(&found, |_| at(10), 600), Vec::<Pid>::new());
}
