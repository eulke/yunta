//! The scripted CLIs' own promises.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use yunta_core::process::signal::{signal_process, Signal};
use yunta_core::Pid;

use super::{claude_code, codex};
use crate::adapter::child_pid_fifo;

/// How long a stub has to start the child it waits on: far longer than a
/// shell takes to start one, even on a loaded machine.
const STARTS_WITHIN: Duration = Duration::from_secs(30);

/// A directory of this test's own, named for `what` and this process, empty.
fn scratch(what: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("yunta-stub-{what}-{}", std::process::id()));
    // best-effort: a directory a crashed run of this test left is emptied first.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The processes whose parent is `pid`, as the host's process table says.
fn children(pid: u32) -> Vec<Pid> {
    let listed = Command::new("pgrep")
        .args(["-P", &pid.to_string()])
        .output()
        .expect("pgrep runs");
    String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .filter_map(|pid| Pid::try_from(pid).ok())
        .collect()
}

/// What `dir` holds, by name.
fn held(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

/// What `stub`, run under the knobs `prefix` names, leaves in the `TMPDIR` it
/// was given once it is killed while it waits for its child to reach their
/// rendezvous. Nobody reads the fifo the child writes its pid to, so the
/// child blocks there and never reaches it.
fn left_when_killed_waiting(stub: &Path, prefix: &str) -> Vec<String> {
    let tmp = scratch(&format!("{prefix}-tmp"));
    let knobs = scratch(&format!("{prefix}-knobs"));
    let fifo = child_pid_fifo(&knobs);
    let mut stub = Command::new(stub)
        .env("TMPDIR", &tmp)
        .env(format!("{prefix}_GROUP_CHILD"), &fifo)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + STARTS_WITHIN;
    let started = loop {
        let started = children(stub.id());
        if !started.is_empty() {
            break started;
        }
        assert!(
            Instant::now() < deadline,
            "{prefix}: the stub started no child"
        );
        std::thread::yield_now();
    };

    for child in started {
        // best-effort: a child already gone is what the kill wanted.
        let _ = signal_process(child, Signal::SIGKILL);
    }
    stub.kill().unwrap();
    stub.wait().unwrap();

    let left = held(&tmp);
    std::fs::remove_dir_all(&tmp).unwrap();
    std::fs::remove_dir_all(&knobs).unwrap();
    left
}

/// A Claude Code stub killed while it waits for its child to reach their
/// rendezvous leaves nothing in the `TMPDIR` it was given. The contract suite
/// kills a session the moment its child's pid arrives, which is often before
/// the child reaches the rendezvous, so a waiting fifo kept there piled up in
/// every reader's `TMPDIR`.
#[test]
fn a_claude_code_stub_killed_while_it_waits_leaves_nothing_in_its_tmpdir() {
    let left = left_when_killed_waiting(&claude_code(), "CLAUDE_STUB");
    assert!(left.is_empty(), "the stub left {left:?} in its TMPDIR");
}

/// A Codex stub killed while it waits for its child to reach their
/// rendezvous leaves nothing in the `TMPDIR` it was given, as a Claude Code
/// stub leaves nothing.
#[test]
fn a_codex_stub_killed_while_it_waits_leaves_nothing_in_its_tmpdir() {
    let left = left_when_killed_waiting(&codex(), "CODEX_STUB");
    assert!(left.is_empty(), "the stub left {left:?} in its TMPDIR");
}
