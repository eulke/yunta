//! A process group closed while its members leave it. A detached daemon —
//! the `gc` or `maintenance` a git command starts — calls `setsid` and
//! runs on in a session of its own; whatever moment the close catches it
//! in, it is never left stopped outside the group, where nothing would
//! continue or kill it.

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use yunta_core::process::group::force_kill_group;
use yunta_core::process::signal::{signal_process, Signal};
use yunta_core::Pid;

/// Starts members one after another, each of which names itself with a
/// file in the directory it is given — while it is still in the group —
/// then leaves the group for a session of its own and sleeps.
const ESCAPE: &str = r#"use POSIX; for (1..200) { my $p = fork(); if (defined $p && $p == 0) { open(my $f, ">", "$ARGV[0]/$$"); close $f; setsid(); sleep 5; exit 0 } } sleep 5"#;

#[tokio::test]
async fn a_member_that_leaves_the_group_as_it_closes_is_never_left_stopped() {
    let escaped = tempfile::tempdir().unwrap();
    for _ in 0..10 {
        close_while_members_escape(escaped.path()).await;
    }

    let pids = escapees(escaped.path()).await;
    let stopped: Vec<&Pid> = pids.iter().filter(|pid| is_stopped(**pid)).collect();
    for pid in &pids {
        let _ = signal_process(*pid, Signal::SIGKILL);
    }
    assert!(!pids.is_empty(), "no member ever started");
    assert!(
        stopped.is_empty(),
        "left stopped outside the group: {stopped:?}"
    );
}

/// Starts a group whose members keep leaving it, and closes the group
/// once the first has started.
async fn close_while_members_escape(escaped: &Path) {
    let script = format!("perl -e '{ESCAPE}' {}; sleep 5", escaped.display());
    let mut leader = Command::new("sh")
        .args(["-c", &script])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pgid = Pid::try_from(leader.id()).unwrap();
    let before = escapees(escaped).await.len();
    tokio::time::timeout(Duration::from_secs(10), async {
        while escapees(escaped).await.len() == before {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("a member starts");

    force_kill_group(pgid).await.unwrap();
    leader.wait().unwrap();
}

/// Every member that named itself.
async fn escapees(dir: &Path) -> Vec<Pid> {
    let mut found = Vec::new();
    let mut entries = tokio::fs::read_dir(dir).await.unwrap();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        if let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
            .and_then(|pid| Pid::try_from(pid).ok())
        {
            found.push(pid);
        }
    }
    found
}

/// Whether `ps` sees `pid` stopped.
fn is_stopped(pid: Pid) -> bool {
    let said = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&said.stdout)
        .trim()
        .starts_with('T')
}
