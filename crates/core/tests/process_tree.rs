//! The groups under a process group, walked by parent links as a CLI's tool
//! commands hang from it. Each command is a real process that writes its own
//! pid and group to a fifo once it has set its group, so what a test expects
//! comes from the process itself, apart from the walk.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use yunta_testkit_core::adapter::{child_pid_fifo, grandchild_pid};

use yunta_core::process::group::group_runs;
use yunta_core::process::signal::{liveness, signal_group, Liveness, Signal};
use yunta_core::process::tree::{close_groups_under, groups_under};
use yunta_core::Pid;

/// Long enough to outlive any test here; every test kills what it started.
const LONG: u32 = 600;

/// A command that sets its group to `group` (0 for one of its own), writes
/// `<pid> <group>` to the fifo `report`, and then sleeps as `sleep LONG`.
fn command(group: i32, report: &Path) -> String {
    format!(
        "perl -e 'setpgrp(0, {group}) or die; open(my $f, \">\", $ARGV[0]) or die; print $f \"$$ \" . getpgrp(); close $f; exec \"sleep\", \"{LONG}\"' {report}",
        report = report.display()
    )
}

/// What a command wrote of itself once its group was set: its pid and group.
async fn reported(report: &Path) -> (i32, i32) {
    let text = grandchild_pid(report).await;
    let mut fields = text.split_whitespace().map(|field| field.parse().unwrap());
    (fields.next().unwrap(), fields.next().unwrap())
}

fn pid(raw: i32) -> Pid {
    Pid::try_from(raw as u32).unwrap()
}

/// Waits, on the process table, until nothing of the group `raw` can run. A
/// member killed under a parent that never reaps it stays a zombie, which
/// runs nothing. The deadline turns a survivor into a failed test.
async fn wait_until_gone(raw: i32) {
    let end = Instant::now() + Duration::from_secs(20);
    while group_runs(pid(raw)).unwrap() {
        assert!(Instant::now() < end, "process group {raw} still runs");
        tokio::task::yield_now().await;
    }
}

/// A shell script run in a process group of its own, as a session's CLI is,
/// in a directory of its own, with every group it names killed when the test
/// ends, passed or not.
struct Leader {
    child: Child,
    dir: tempfile::TempDir,
    kill: Vec<i32>,
}

impl Leader {
    fn start(script: impl FnOnce(&Path) -> String) -> Self {
        let dir = tempfile::tempdir().unwrap();
        child_pid_fifo(dir.path());
        let child = Command::new("sh")
            .args(["-c", &script(dir.path())])
            .process_group(0)
            .spawn()
            .unwrap();
        let group = child.id() as i32;
        Leader {
            child,
            dir,
            kill: vec![group],
        }
    }

    fn group(&self) -> Pid {
        Pid::try_from(self.child.id()).unwrap()
    }

    fn report(&self) -> PathBuf {
        self.dir.path().join("child.pid")
    }
}

impl Drop for Leader {
    fn drop(&mut self) {
        for group in &self.kill {
            if let Ok(group) = Pid::try_from(*group as u32) {
                // best-effort: a group already gone is what the test wanted.
                let _ = signal_group(group, Signal::SIGKILL);
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A command in a group of its own, under a shell under the leader: the
/// walk names its group.
#[tokio::test]
async fn the_walk_finds_a_grandchild_in_a_group_of_its_own() {
    let mut leader = Leader::start(|dir| format!("{} & wait", command(0, &dir.join("child.pid"))));
    let (_, group) = reported(&leader.report()).await;
    leader.kill.push(group);

    let groups = groups_under(leader.group()).unwrap();

    assert_ne!(group, leader.group().as_i32());
    assert_eq!(groups, vec![pid(group)]);
}

/// A leader that exited leaves a member of its group running, and the walk
/// still finds the group that member made.
#[tokio::test]
async fn the_walk_starts_from_every_member_once_the_leader_left() {
    let mut leader =
        Leader::start(|dir| format!("({} & wait) & exit 0", command(0, &dir.join("child.pid"))));
    let (_, group) = reported(&leader.report()).await;
    leader.kill.push(group);
    leader.child.wait().unwrap();

    let groups = groups_under(leader.group()).unwrap();

    assert_eq!(groups, vec![pid(group)]);
}

/// A leader that started nothing has nothing under it.
#[tokio::test]
async fn the_walk_finds_nothing_under_a_leader_with_no_children() {
    let leader = Leader::start(|dir| {
        format!(
            "echo \"$$ $$\" > {report} && exec sleep {LONG}",
            report = dir.join("child.pid").display()
        )
    });
    reported(&leader.report()).await;

    assert_eq!(groups_under(leader.group()).unwrap(), Vec::new());
}

/// A child that joins the caller's own group is never named: closing it
/// would close the caller.
#[tokio::test]
async fn the_walk_never_names_the_callers_group() {
    let own = rustix::process::getpgrp().as_raw_pid();
    let leader = Leader::start(|dir| format!("{} & wait", command(own, &dir.join("child.pid"))));
    let (joined, group) = reported(&leader.report()).await;

    let groups = groups_under(leader.group()).unwrap();

    // Killed by its own pid: its group is this test's.
    let _ = Command::new("kill")
        .args(["-9", &joined.to_string()])
        .status();
    assert_eq!(group, own, "the child did not join the caller's group");
    assert_eq!(groups, Vec::new());
}

/// Closing the groups under a leader leaves none of them running, and the
/// leader's own group untouched.
#[tokio::test]
async fn closing_the_groups_under_a_leader_leaves_its_own_group_running() {
    let mut leader =
        Leader::start(|dir| format!("{} & exec sleep {LONG}", command(0, &dir.join("child.pid"))));
    let (_, group) = reported(&leader.report()).await;
    leader.kill.push(group);

    close_groups_under(leader.group()).await.unwrap();

    wait_until_gone(group).await;
    assert_eq!(liveness(leader.group()), Liveness::Alive);
}
