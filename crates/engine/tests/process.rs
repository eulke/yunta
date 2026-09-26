//! `spawn_governed`: every subprocess the engine runs is born in its own
//! process group, bounded by its timeout and its cancellation, and dies
//! with its whole tree; its pipes are read to the end on every path, so
//! the outcome always carries what the child wrote.

use std::time::Duration;

use tokio_util::sync::CancellationToken;
use yunta_core::Pid;
use yunta_engine::process::{spawn_governed, GovernedCommand, Outcome, Supervision};
use yunta_testkit::Owner;
use yunta_testkit_core::FixedClock;

struct CancelOnDrop(CancellationToken);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

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

/// Outside a run there is no registry, but there is always somebody who
/// can stop the work: a supervision that cannot be built without a token
/// is what makes "no subprocess without an owner" a property of the type
/// rather than of every call site remembering.
#[tokio::test]
async fn a_supervision_outside_any_run_still_answers_to_its_token() {
    let dir = tempfile::tempdir().unwrap();
    let owner = Owner::new();
    let marker = dir.path().join("running.marker");
    let trigger = owner.cancellation().clone();
    let watcher = tokio::spawn(async move {
        while !marker.exists() {
            tokio::task::yield_now().await;
        }
        trigger.cancel();
    });

    let outcome = spawn_governed(
        GovernedCommand::shell(dir.path(), "touch running.marker; tail -f /dev/null"),
        owner.supervision(),
    )
    .await
    .unwrap();
    watcher.await.unwrap();

    let Outcome::Cancelled { pgid, .. } = outcome else {
        panic!("expected a cancellation, got {outcome:?}");
    };
    assert!(
        !group_running(pgid),
        "a command outside any run dies with its tree like any other"
    );
}

#[tokio::test]
async fn context_command_timeout_leaves_no_process_alive() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    let command = GovernedCommand::shell(
        dir.path(),
        "echo started; tail -f /dev/null & tail -f /dev/null",
    )
    .timeout(Duration::from_millis(200));

    let outcome = spawn_governed(command, owner.supervision()).await.unwrap();

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

    let outcome = spawn_governed(command, Supervision::outside_any_run(&cancel, &FixedClock))
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
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    let command = GovernedCommand::shell(dir.path(), "echo out; echo err 1>&2; exit 3");

    let outcome = spawn_governed(command, owner.supervision()).await.unwrap();

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

#[tokio::test]
async fn a_finished_shell_does_not_leave_a_descendant_holding_its_pipes_open() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    let command = GovernedCommand::shell(
        dir.path(),
        "sleep 30 & printf 'leader stdout\\n'; printf 'leader stderr\\n' >&2; exit 0",
    );

    let outcome = tokio::time::timeout(
        Duration::from_secs(3),
        spawn_governed(command, owner.supervision()),
    )
    .await
    .expect("closing the descendant also closes the inherited pipes")
    .unwrap();

    let Outcome::Exited {
        status,
        stdout,
        stderr,
    } = outcome
    else {
        panic!("expected the shell's successful exit, got {outcome:?}");
    };
    assert!(status.success());
    assert_eq!(stdout, b"leader stdout\n");
    assert_eq!(stderr, b"leader stderr\n");
}

#[tokio::test]
async fn large_input_and_both_output_streams_are_drained_concurrently() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    let input = vec![b'i'; 1024 * 1024];
    let command = GovernedCommand::shell(
        dir.path(),
        "cat >/dev/null & dd if=/dev/zero bs=65536 count=16 2>/dev/null; dd if=/dev/zero bs=65536 count=16 1>&2 2>/dev/null; wait",
    )
    .stdin(input);

    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        spawn_governed(command, owner.supervision()),
    )
    .await
    .expect("stdin and both output pipes make progress together")
    .unwrap();

    let Outcome::Exited {
        status,
        stdout,
        stderr,
    } = outcome
    else {
        panic!("expected a successful exit, got {outcome:?}");
    };
    assert!(status.success());
    assert_eq!(stdout.len(), 1024 * 1024);
    assert!(stdout.iter().all(|byte| *byte == 0));
    assert_eq!(stderr.len(), 1024 * 1024);
    assert!(stderr.iter().all(|byte| *byte == 0));
}

#[tokio::test]
async fn early_stdin_close_is_not_reported_as_a_pipe_failure() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    let command = GovernedCommand::shell(dir.path(), "exit 0").stdin(vec![b'x'; 8 * 1024 * 1024]);

    let outcome = tokio::time::timeout(
        Duration::from_secs(3),
        spawn_governed(command, owner.supervision()),
    )
    .await
    .expect("a closed stdin does not strand the writer task")
    .unwrap();

    let Outcome::Exited { status, .. } = outcome else {
        panic!("expected the child's successful exit, got {outcome:?}");
    };
    assert!(status.success());
}

#[tokio::test]
async fn repeated_forks_during_cancellation_leave_no_running_group_members() {
    for iteration in 0..12 {
        let dir = tempfile::tempdir().unwrap();
        let children = dir.path().join("children");
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let observed = children.clone();
        let watcher = tokio::spawn(async move {
            let _cancel_on_drop = CancelOnDrop(trigger);
            yunta_testkit::wait_until_async(
                || async {
                    tokio::fs::read_to_string(&observed)
                        .await
                        .map(|lines| lines.lines().count() >= 3)
                        .unwrap_or_default()
                },
                || format!("iteration {iteration}: the shell did not create three children"),
            )
            .await;
        });
        let command = GovernedCommand::shell(
            dir.path(),
            "while :; do sleep 30 & echo $! >> \"$1\"; sleep 0.01; done",
        )
        .arg("sh")
        .arg(children.display().to_string());
        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            spawn_governed(command, Supervision::outside_any_run(&cancel, &FixedClock)),
        )
        .await
        .unwrap_or_else(|_| panic!("iteration {iteration}: cancellation did not close the group"))
        .unwrap();
        watcher.await.unwrap();

        let Outcome::Cancelled { pgid, .. } = outcome else {
            panic!("iteration {iteration}: expected cancellation, got {outcome:?}");
        };
        assert!(
            !group_running(pgid),
            "iteration {iteration}: group {pgid} still has an executable member"
        );
    }
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
    // The stub publishes its pid by renaming a file it has already
    // written, never by writing the file the test watches: a rename is
    // atomic, so the moment the path exists it holds the whole pid, and
    // the watcher below needs no interval to be sure of that.
    std::fs::write(
        bin.join("git"),
        format!(
            "#!/bin/sh\necho $$ > {0}.partial\nmv {0}.partial {0}\ntail -f /dev/null\n",
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
    let supervision = Supervision::outside_any_run(&cancel, &FixedClock).with_env(&env);

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

#[test]
fn a_corrupt_registry_is_reported_as_corrupt_not_absent() {
    // An `engine.json` that is there and will not read is a fact about
    // this run: a reader told it was absent would conclude the engine
    // never wrote one, which is a different thing to do about it.
    let run_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(run_dir.path().join("scratch")).unwrap();
    std::fs::write(
        yunta_engine::registry_path(run_dir.path()),
        "{ this is not a registry",
    )
    .unwrap();

    match yunta_engine::read_registry(run_dir.path()) {
        yunta_engine::Registry::Corrupt(error) => {
            let said = yunta_core::describe(&error);
            assert!(
                said.contains("process registry"),
                "the refusal names what the file was meant to be: {said}"
            );
        }
        yunta_engine::Registry::Absent => {
            panic!("a file that is there is not absent")
        }
        yunta_engine::Registry::Read(_) => panic!("that is not a registry"),
    }

    // And a run with no registry at all still reads as absent.
    let empty = tempfile::tempdir().unwrap();
    assert!(matches!(
        yunta_engine::read_registry(empty.path()),
        yunta_engine::Registry::Absent
    ));
}
