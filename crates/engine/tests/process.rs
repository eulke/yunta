//! `spawn_governed`: every subprocess the engine runs is born in its own
//! process group, bounded by its timeout and its cancellation, and dies
//! with its whole tree; its pipes are read to the end on every path, so
//! the outcome always carries what the child wrote.

use std::time::Duration;

use yunta_core::Pid;
use yunta_engine::process::{spawn_governed, GovernedCommand, Outcome, Supervision};

/// True while any member of `pgid` still runs. A zombie is not
/// running: it has exited and only waits for its parent to collect its
/// status, and the parent of an orphaned member is the host's init, not
/// the engine — so `kill -0`, which counts zombies, would report a
/// killed group as alive on a host whose init reaps lazily.
fn group_running(pgid: Pid) -> bool {
    let listing = std::process::Command::new("ps")
        .args(["-e", "-o", "pid=,pgid=,stat="])
        .output()
        .expect("ps lists every process");
    let group = pgid.to_string();
    String::from_utf8_lossy(&listing.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let _pid = fields.next()?;
            Some((fields.next()?, fields.next()?))
        })
        .any(|(member_group, state)| member_group == group && !state.starts_with('Z'))
}

#[tokio::test]
async fn context_command_timeout_leaves_no_process_alive() {
    let dir = tempfile::tempdir().unwrap();
    let command = GovernedCommand::shell(dir.path(), "echo started; sleep 30 & sleep 30")
        .timeout(Duration::from_millis(200));

    let outcome = spawn_governed(command, Supervision::none()).await.unwrap();

    let Outcome::TimedOut { pgid, stdout, .. } = outcome else {
        panic!("expected a timeout, got {outcome:?}");
    };
    assert_eq!(
        String::from_utf8_lossy(&stdout),
        "started\n",
        "the reader was awaited before the outcome was reported"
    );
    assert!(
        !group_running(pgid),
        "process group {pgid} still has a running member after the timeout"
    );
}

#[tokio::test]
async fn cancellation_kills_the_tree_and_drains_the_pipes() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        trigger.cancel();
    });
    let command = GovernedCommand::shell(dir.path(), "echo begun; sleep 30 & sleep 30");

    let outcome = spawn_governed(
        command,
        Supervision {
            registry: None,
            cancel: Some(&cancel),
            env: &[],
        },
    )
    .await
    .unwrap();

    let Outcome::Cancelled { pgid, stdout, .. } = outcome else {
        panic!("expected a cancellation, got {outcome:?}");
    };
    assert_eq!(String::from_utf8_lossy(&stdout), "begun\n");
    assert!(
        !group_running(pgid),
        "process group {pgid} still has a running member after the cancellation"
    );
}

#[tokio::test]
async fn a_finished_command_reports_its_status_and_both_streams() {
    let dir = tempfile::tempdir().unwrap();
    let command = GovernedCommand::shell(dir.path(), "echo out; echo err 1>&2; exit 3");

    let outcome = spawn_governed(command, Supervision::none()).await.unwrap();

    let Outcome::Exited {
        status,
        stdout,
        stderr,
    } = outcome
    else {
        panic!("expected an exit, got {outcome:?}");
    };
    assert_eq!(status.code(), Some(3));
    assert_eq!(String::from_utf8_lossy(&stdout), "out\n");
    assert_eq!(String::from_utf8_lossy(&stderr), "err\n");
}
