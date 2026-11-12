use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use super::*;
use crate::process::signal::{liveness, Liveness};

#[tokio::test]
async fn force_kill_group_kills_a_live_group() {
    let mut child = Command::new("sh")
        .args(["-c", "sleep 30 & wait"])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pgid = Pid::try_from(child.id()).unwrap();

    force_kill_group(pgid).await.unwrap();
    assert!(inspect(pgid)
        .unwrap()
        .iter()
        .all(|member| member.state == State::Terminated));
    child.wait().unwrap();
}

/// After its signal a kill looks again within a millisecond and backs
/// off to one interval, so a group that dies at its signal is seen
/// closed long before a whole interval, and one that will not die is
/// still watched for about a second.
#[test]
fn a_kill_looks_again_soon_after_its_signal_then_backs_off() {
    let first: Vec<Duration> = waits().take(6).collect();
    assert_eq!(first, [1, 2, 4, 8, 10, 10].map(Duration::from_millis));
    let watched: Duration = waits().take(MAX_OBSERVATIONS - 1).sum();
    assert!(
        (Duration::from_millis(950)..=Duration::from_secs(1)).contains(&watched),
        "{watched:?}"
    );
}

#[tokio::test]
async fn exit_observation_keeps_the_leader_waitable_until_group_cleanup() {
    let mut child = Command::new("sh")
        .args(["-c", "exit 0"])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pgid = Pid::try_from(child.id()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if child_has_exited_unreaped(pgid).unwrap() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the child exits");

    assert!(child_has_exited_unreaped(pgid).unwrap());
    assert_eq!(
        liveness(pgid),
        Liveness::Alive,
        "WNOWAIT leaves the zombie waitable"
    );
    child.wait().unwrap();
    assert_eq!(
        liveness(pgid),
        Liveness::Dead,
        "the ordinary wait reaps the leader"
    );
}

#[tokio::test]
async fn a_leader_that_exited_leaving_a_child_still_has_its_child_killed() {
    let mut child = Command::new("sh")
        .args(["-c", "sleep 30 & exit 0"])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pgid = Pid::try_from(child.id()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !child_has_exited_unreaped(pgid).unwrap() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the leader exits");
    assert!(
        inspect(pgid)
            .unwrap()
            .iter()
            .any(|member| member.state == State::Running),
        "the sleep it left behind runs on in its group"
    );

    force_kill_group(pgid).await.unwrap();

    assert!(inspect(pgid)
        .unwrap()
        .iter()
        .all(|member| member.state == State::Terminated));
    child.wait().unwrap();
}

/// A group whose members keep forking is emptied all the same: a
/// child born while one signal was delivered gets the next.
#[tokio::test]
async fn a_group_that_keeps_forking_is_emptied() {
    let mut child = Command::new("sh")
        .args(["-c", "while :; do sleep 30 & done"])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pgid = Pid::try_from(child.id()).unwrap();

    force_kill_group(pgid).await.unwrap();

    assert!(inspect(pgid)
        .unwrap()
        .iter()
        .all(|member| member.state == State::Terminated));
    child.wait().unwrap();
}

#[tokio::test]
async fn an_empty_group_needs_no_signal() {
    let pgid = Pid::try_from(999_999_999_u32).unwrap();
    force_kill_group(pgid).await.unwrap();
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn an_empty_group_ignores_errno_left_by_an_unrelated_call() {
    errno::set_errno(errno::Errno(316));
    let pgid = Pid::try_from(999_999_999_u32).unwrap();
    force_kill_group(pgid).await.unwrap();
}

#[tokio::test]
async fn zombies_are_not_running_group_members() {
    let mut child = Command::new("sh")
        .args(["-c", "exit 0"])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pgid = Pid::try_from(child.id()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !child_has_exited_unreaped(pgid).unwrap() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the child exits");

    assert!(inspect(pgid)
        .unwrap()
        .iter()
        .all(|member| member.state == State::Terminated));
    assert_eq!(
        liveness(pgid),
        Liveness::Alive,
        "the child remains waitable"
    );
    child.wait().unwrap();
}

/// A reaped pid reads as dead through errno. The macOS inspection takes the
/// same answer instead of matching libproc's error text.
#[test]
fn a_reaped_pid_reads_as_dead_by_errno() {
    let mut child = Command::new("true").spawn().unwrap();
    let pid = Pid::try_from(child.id()).unwrap();
    child.wait().unwrap();
    assert_eq!(liveness(pid), Liveness::Dead);
}
