use futures::StreamExt;
use std::collections::BTreeMap;
use std::path::Path;
use yunta_core::port::session::SessionEnv;
use yunta_core::port::AgentEvent;
use yunta_core::process::mark::SessionMark;
use yunta_core::process::signal::{liveness, signal_process, Liveness, Signal};
use yunta_core::process::subprocess::{self, Launch, LineParser};
use yunta_core::process::SearchPath;
use yunta_core::{AdapterId, Pid, RunId};
use yunta_testkit_core::adapter::{child_pid_fifo, grandchild_pid, wait_until_gone};

/// A parser that fails on the first line it is given, so its reader ends
/// after the session's output has begun.
struct FailsOnFirstLine;

impl LineParser for FailsOnFirstLine {
    fn parse(&mut self, _line: &str) -> Vec<AgentEvent> {
        panic!("the parser fails on its first line")
    }
}

#[tokio::test]
async fn a_failed_reader_does_not_make_the_kill_of_a_closed_group_fail() {
    static ADAPTER: AdapterId = AdapterId::from_static("probe");
    let dir = tempfile::tempdir().unwrap();
    let env = SessionEnv {
        plain: BTreeMap::new(),
        secret: BTreeMap::new(),
    };
    let mut session = subprocess::open(Launch {
        adapter: &ADAPTER,
        binary: Path::new("/bin/sh"),
        search_path: &SearchPath::default(),
        args: vec!["-c".into(), "echo first; sleep 30".into()],
        cwd: dir.path(),
        env: &env,
        mark: &yunta_core::process::mark::SessionMark::new(&yunta_core::RunId::from(
            "01HZYXW8M6D4V2T0R9Q7P5N3K3",
        )),
        prompt: "",
        parser: Box::new(FailsOnFirstLine),
        held: None,
    })
    .await
    .unwrap();
    let pgid = session
        .pgid()
        .expect("a session that just opened names its process group");
    // The stream ends when the reader drops its sender, which a panicked reader does.
    session.events().collect::<Vec<AgentEvent>>().await;
    session
        .kill()
        .await
        .expect("the group closed, so the kill answers ok");
    assert_eq!(liveness(pgid), Liveness::Dead);
}

/// A parser that reads nothing: the session below says nothing it needs.
struct ReadsNothing;

impl LineParser for ReadsNothing {
    fn parse(&mut self, _line: &str) -> Vec<AgentEvent> {
        Vec::new()
    }
}

/// Processes a test started, killed when it ends, passed or not.
struct Started(Vec<Pid>);

impl Drop for Started {
    fn drop(&mut self) {
        for pid in &self.0 {
            // best-effort: a process already gone is what the test wanted.
            let _ = signal_process(*pid, Signal::SIGKILL);
        }
    }
}

/// The CLI a dropped session below runs: a shell that starts a command in a
/// group of its own under itself and one whose parent leaves at once, each
/// writing its pid to its fifo, and then waits.
fn starts_two_commands(under: &Path, orphaned: &Path) -> String {
    format!(
        "perl -e 'setpgrp(0, 0); open(my $f, \">\", $ARGV[0]) or die; print $f $$; close $f; exec \"sleep\", \"600\"' {under} < /dev/null > /dev/null 2>&1 &\n\
         sh -c 'python3 -c \"import os, sys, time; os.setpgrp(); f = open(sys.argv[1], \\\"w\\\"); f.write(str(os.getpid())); f.close(); time.sleep(600)\" \"$0\" < /dev/null > /dev/null 2>&1 &' {orphaned}\n\
         exec sleep 30",
        under = under.display(),
        orphaned = orphaned.display()
    )
}

/// A session nobody ended, dropped, leaves nothing its CLI started: neither a
/// command in a group of its own under the CLI nor one whose parent left.
#[tokio::test]
async fn a_dropped_session_leaves_no_command_behind() {
    static ADAPTER: AdapterId = AdapterId::from_static("probe");
    let dir = tempfile::tempdir().unwrap();
    let (under_dir, orphaned_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let under = child_pid_fifo(under_dir.path());
    let orphaned = child_pid_fifo(orphaned_dir.path());
    let env = SessionEnv::default();
    let mark = SessionMark::new(&RunId::from("01HZYXW8M6D4V2T0R9Q7P5N3K5"));
    let session = subprocess::open(Launch {
        adapter: &ADAPTER,
        binary: Path::new("/bin/sh"),
        search_path: &SearchPath::default(),
        args: vec!["-c".into(), starts_two_commands(&under, &orphaned)],
        cwd: dir.path(),
        env: &env,
        mark: &mark,
        prompt: "",
        parser: Box::new(ReadsNothing),
        held: None,
    })
    .await
    .unwrap();
    let mut started = Started(Vec::new());
    for fifo in [&under, &orphaned] {
        let pid: u32 = grandchild_pid(fifo).await.parse().unwrap();
        started.0.push(Pid::try_from(pid).unwrap());
    }

    drop(session);

    for pid in &started.0 {
        wait_until_gone(*pid).await;
    }
}
