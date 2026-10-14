//! A process group runs while a child its leader left in it runs, even after
//! the leader has exited and been reaped. `liveness` reads the group id as the
//! leader's pid, so it reports such a group gone; `group_runs` reads every
//! member.

use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use yunta_core::process::group::{force_kill_group, group_runs};
use yunta_core::process::signal::{liveness, Liveness};
use yunta_core::Pid;

#[tokio::test]
async fn a_group_whose_leader_exited_runs_while_a_child_it_left_runs() {
    let mut leader = Command::new("sh")
        .args(["-c", "sleep 30 & exit 0"])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pgid = Pid::try_from(leader.id()).unwrap();
    leader.wait().unwrap();

    assert!(
        group_runs(pgid).unwrap(),
        "the sleep the leader left behind runs in the group"
    );
    assert_ne!(
        liveness(pgid),
        Liveness::Alive,
        "the reaped leader names no process"
    );

    // Reaped first, as `yunta cancel` finds the group: the registry names it
    // once the engine that spawned the leader has exited.
    force_kill_group(pgid).await.unwrap();

    assert!(
        !group_runs(pgid).unwrap(),
        "a killed group has no member that runs"
    );
}
