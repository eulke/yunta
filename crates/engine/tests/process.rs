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
    let command = GovernedCommand::shell(
        dir.path(),
        "echo started; tail -f /dev/null & tail -f /dev/null",
    )
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
    let marker = dir.path().join("running.marker");
    tokio::spawn(async move {
        // Cancel once the command is actually running: it writes the marker
        // before it blocks, so the cancellation lands on a live process tree
        // rather than after a fixed delay.
        while !marker.exists() {
            tokio::task::yield_now().await;
        }
        trigger.cancel();
    });
    let command = GovernedCommand::shell(
        dir.path(),
        "echo begun; touch running.marker; tail -f /dev/null & tail -f /dev/null",
    );

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

/// A `git` a run spawns is a subprocess the run owns: born in its own
/// process group, registered, and killed with its whole tree when the run
/// is cancelled. Proven with a `git` of the test's own on an injected
/// `PATH`, so the assertion is about who governs the child rather than
/// about what real git does.
#[tokio::test]
async fn a_cancelled_run_kills_the_git_it_spawned() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let started = dir.path().join("started");
    std::fs::write(
        bin.join("git"),
        format!(
            "#!/bin/sh\necho $$ > {}\ntail -f /dev/null\n",
            started.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(bin.join("git"), std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let cancel = tokio_util::sync::CancellationToken::new();
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let env = [("PATH".to_string(), path.clone())];
    let supervision = Supervision {
        registry: None,
        cancel: Some(&cancel),
        env: &env,
    };

    let waiting = started.clone();
    let trigger = cancel.clone();
    let killer = tokio::spawn(async move {
        // The stub is running once it has written its pid.
        while !waiting.exists() {
            tokio::task::yield_now().await;
        }
        trigger.cancel();
    });

    let error = yunta_engine::git::output(dir.path(), &["status"], supervision)
        .await
        .expect_err("a cancelled git never answers");
    killer.await.unwrap();

    let pid: i32 = std::fs::read_to_string(&started)
        .expect("the stub ran")
        .trim()
        .parse()
        .expect("the stub wrote its pid");
    assert!(
        !group_running(Pid::try_from(pid).expect("a real pid")),
        "the git this run spawned outlived the run's cancellation: {error}"
    );
}
