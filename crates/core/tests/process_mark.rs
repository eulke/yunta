//! The sweep by mark: a process a session's CLI started, left to pid 1 by a
//! parent that exited, is found by the mark it inherited and closed. Each
//! process here is a real orphan that writes its pid to a fifo once it runs,
//! so what a test expects comes from the process itself, apart from the
//! sweep.

use std::process::Command;
use std::time::{Duration, SystemTime};

use yunta_core::process::mark::{close_marked, marked, SessionMark, MARK_VAR};
use yunta_core::process::signal::{liveness, signal_process, Liveness, Signal};
use yunta_core::{Pid, RunId};
use yunta_testkit_core::adapter::{child_pid_fifo, grandchild_pid, wait_until_gone};

/// Long enough to outlive any test here; every test kills what it started.
const LONG: u32 = 600;

/// Processes a test started, killed when it ends, passed or not.
#[derive(Default)]
struct Started(Vec<Pid>);

impl Drop for Started {
    fn drop(&mut self) {
        for pid in &self.0 {
            // best-effort: a process already gone is what the test wanted.
            let _ = signal_process(*pid, Signal::SIGKILL);
        }
    }
}

/// Starts `argv` from a shell that exits at once, so its parent is gone, with
/// `mark` in its environment when there is one, and answers its pid once it
/// runs. The command writes its own pid to the fifo `argv` is given last.
async fn orphan(started: &mut Started, argv: &str, mark: Option<&str>) -> Pid {
    let dir = tempfile::tempdir().unwrap();
    let fifo = child_pid_fifo(dir.path());
    let mut shell = Command::new("sh");
    shell.args(["-c", &format!("{argv} {} &", fifo.display())]);
    shell.env_remove(MARK_VAR);
    if let Some(mark) = mark {
        shell.env(MARK_VAR, mark);
    }
    shell.status().unwrap();
    let pid: u32 = grandchild_pid(&fifo).await.parse().unwrap();
    let pid = Pid::try_from(pid).unwrap();
    started.0.push(pid);
    pid
}

/// A `python3` in a group of its own that writes its pid and then sleeps:
/// macOS lets another process read a `python3`'s environment.
fn python() -> String {
    format!(
        "python3 -c 'import os, sys, time; os.setpgrp(); f = open(sys.argv[1], \"w\"); f.write(str(os.getpid())); f.close(); time.sleep({LONG})'"
    )
}

fn run(id: &str) -> RunId {
    RunId::from(id)
}

/// A process carrying the session's mark whose parent left is found and
/// closed.
#[tokio::test]
async fn the_sweep_finds_a_marked_process_whose_parent_left() {
    let mut started = Started::default();
    let mark = SessionMark::new(&run("01KSWEEPFINDSANORPHAN00001"));
    let since = SystemTime::now();
    let left = orphan(&mut started, &python(), Some(mark.as_str())).await;

    close_marked(&mark.marks(since)).await.unwrap();

    wait_until_gone(left).await;
}

/// A process with another session's mark, and one with none, are left
/// running.
#[tokio::test]
async fn the_sweep_leaves_another_sessions_mark_and_no_mark_running() {
    let mut started = Started::default();
    let run_id = run("01KSWEEPLEAVESOTHERS000001");
    let mark = SessionMark::new(&run_id);
    let other = SessionMark::new(&run_id);
    let since = SystemTime::now();
    let with_other = orphan(&mut started, &python(), Some(other.as_str())).await;
    let with_none = orphan(&mut started, &python(), None).await;

    close_marked(&mark.marks(since)).await.unwrap();

    assert_eq!(liveness(with_other), Liveness::Alive);
    assert_eq!(liveness(with_none), Liveness::Alive);
}

/// The marks of a run are every session's of that run and no other run's.
#[tokio::test]
async fn the_marks_of_a_run_close_its_sessions_and_no_other_runs() {
    let mut started = Started::default();
    let this = run("01KSWEEPTHISRUN00000000001");
    let that = run("01KSWEEPTHISRUN00000000002");
    let first = orphan(
        &mut started,
        &python(),
        Some(SessionMark::new(&this).as_str()),
    )
    .await;
    let second = orphan(
        &mut started,
        &python(),
        Some(SessionMark::new(&this).as_str()),
    )
    .await;
    let elsewhere = orphan(
        &mut started,
        &python(),
        Some(SessionMark::new(&that).as_str()),
    )
    .await;

    close_marked(&SessionMark::of_run(&this)).await.unwrap();

    wait_until_gone(first).await;
    wait_until_gone(second).await;
    assert_eq!(liveness(elsewhere), Liveness::Alive);
}

/// Whether `ps` shows `mark` in the environment of `pid`. It reads another
/// process's environment through the host, as the sweep does, so it says what
/// this host shows, apart from the sweep.
fn ps_shows(pid: Pid, mark: &str) -> bool {
    let shown = Command::new("ps")
        .args(["-E", "-ww", "-o", "command=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&shown.stdout).contains(mark)
}

/// macOS can hide the environment of its own platform binaries from every
/// other process: this Mac does, a CI runner did not. Where it does, a marked
/// `/bin/sleep` reads as unmarked and the sweep goes on; where it does not, as
/// on Linux, the sweep finds it.
#[tokio::test]
async fn a_hidden_environment_reads_as_unmarked() {
    let mut started = Started::default();
    let mark = SessionMark::new(&run("01KSWEEPHIDDENENVIRONMENT1"));
    let since = SystemTime::now();
    let sleep = format!("sh -c 'echo $$ > \"$0\"; exec /bin/sleep {LONG}'");
    let hidden = orphan(&mut started, &sleep, Some(mark.as_str())).await;
    let shown = cfg!(target_os = "linux") || ps_shows(hidden, mark.as_str());

    let found = marked(&mark.marks(since)).unwrap();

    let named = found.iter().any(|process| process.pid == hidden);
    assert_eq!(named, shown, "{found:?}");
}

/// A session's sweep reads only what may have inherited its mark: a marked
/// process that started well before the session's CLI is not found by that
/// session's sweep, and the same process is found by a session whose CLI
/// started before it.
#[tokio::test]
async fn a_process_older_than_its_session_is_not_read_for_its_mark() {
    let mut started = Started::default();
    let mark = SessionMark::new(&run("01KSWEEPOLDERTHANSESSION01"));
    let before = SystemTime::now();
    let left = orphan(&mut started, &python(), Some(mark.as_str())).await;
    let an_hour_later = SystemTime::now() + Duration::from_secs(3600);

    let for_later = marked(&mark.marks(an_hour_later)).unwrap();
    let for_before = marked(&mark.marks(before)).unwrap();

    assert!(
        !for_later.iter().any(|process| process.pid == left),
        "{for_later:?}"
    );
    assert!(
        for_before.iter().any(|process| process.pid == left),
        "{for_before:?}"
    );
}
