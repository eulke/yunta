//! What a pipe the drain stops reading does to an outcome: the output a
//! command's own processes wrote is marked as possibly incomplete, rather
//! than reported as the whole of it.

use tokio_util::sync::CancellationToken;
use yunta_engine::process::{spawn_governed, Drain, GovernedCommand, Outcome, Supervision};

#[tokio::test]
async fn output_is_marked_truncated_when_a_pipe_is_held_outside_the_group() {
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left-the-group");
    let go = dir.path().join("go");
    // `perl` leaves the process group and keeps stdout open. The shell stays
    // alive until the test writes `go`, which it does once `perl` has left
    // the group; the drain then gives up after its bound with the pipe still
    // held.
    let script = format!(
        r#"echo partial; perl -MPOSIX -e 'setsid(); open(my $f, ">", $ARGV[0]) or die; close $f; exec("sleep", "10")' '{}' & while [ ! -e '{}' ]; do sleep 0.01; done"#,
        left.display(),
        go.display(),
    );
    let cwd = dir.path().to_path_buf();
    let run = tokio::spawn(async move {
        let cancel = CancellationToken::new();
        spawn_governed(
            GovernedCommand::shell(&cwd, &script),
            Supervision::outside_any_run(&cancel, &yunta_testkit_core::FixedClock),
        )
        .await
    });
    yunta_testkit::wait_until_async(
        || std::future::ready(left.exists()),
        || "the child never left its process group".into(),
    )
    .await;
    tokio::fs::write(&go, "").await.unwrap();
    let outcome = run.await.unwrap().unwrap();
    assert!(
        matches!(outcome, Outcome::Exited { ref stdout, drain: Drain::Truncated, .. } if stdout == b"partial\n"),
        "got {outcome:?}"
    );
}
