//! The lock a run's `none` isolation takes and the owner record it leaves:
//! who holds the lock, when a dead holder's lock is taken, what a live or
//! unverifiable holder keeps, and what is released.

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;
use yunta_core::process::signal::Liveness;
use yunta_core::{Isolation, Pid};
use yunta_engine::lock::{
    acquire, release, Acquired, Contention, LockError, LockOwner, OwnerProbe,
};
use yunta_engine::{prepare_worktree, WorktreeError};
use yunta_testkit::{init_repo, Owner};
use yunta_testkit_core::{fixed_now, FixedClock};

use super::head;

// --- owner-aware `none` lock ------------------------------------------

pub(super) fn lock_file(repo: &Path) -> std::path::PathBuf {
    repo.join(".git/yunta-none.lock")
}

pub(super) fn mutation_lock_file(repo: &Path) -> std::path::PathBuf {
    repo.join(".git/yunta-worktree.lock")
}

/// The record every lock writer produces: the holder's pid and when it
/// took the lock.
pub(super) fn owner_record(pid: u32, started_at: DateTime<Utc>) -> String {
    serde_json::json!({ "pid": pid, "started_at": started_at }).to_string()
}

/// A pid that is certainly dead: a child spawned and reaped here.
pub(super) fn dead_pid() -> u32 {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

/// A probe that answers what the test decides, whatever the pid.
struct Scripted {
    liveness: Liveness,
    started: Option<DateTime<Utc>>,
}

impl OwnerProbe for Scripted {
    fn liveness(&self, _pid: Pid) -> Liveness {
        self.liveness
    }

    fn started(&self, _pid: Pid) -> Option<DateTime<Utc>> {
        self.started
    }
}

pub(super) fn read_owner(path: &Path) -> LockOwner {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// A `keep` that never keeps a dead holder's lock, so its takeover goes ahead.
fn take_over(_: &LockOwner) -> bool {
    false
}

#[tokio::test]
async fn a_dead_owner_s_lock_is_stolen_and_the_takeover_is_reported() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    // A pid that is certainly dead: a child we spawn and reap ourselves.
    let dead = std::process::Command::new("true").spawn().unwrap();
    let dead_pid = dead.id();
    let _ = std::process::Child::wait(&mut { dead });
    std::fs::write(lock_file(&repo), owner_record(dead_pid, fixed_now())).unwrap();

    let prepared = prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap();
    match prepared {
        yunta_engine::WorktreePrepared::StoleStaleLock { dead_pid: reported } => {
            assert_eq!(reported.as_u32(), dead_pid);
        }
        other => panic!("expected the stale lock to be stolen, got {other:?}"),
    }
    // The lock now names this process as its owner.
    let content = std::fs::read_to_string(lock_file(&repo)).unwrap();
    assert!(
        content.contains(&std::process::id().to_string()),
        "got: {content}"
    );
}

#[tokio::test]
async fn a_live_owner_s_lock_still_refuses() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    // This test process itself is the live owner. Its record names the start
    // the host reports for it, since a process that started after its record
    // reads as a reused pid; a host that reports no start reads it as alive.
    let started = yunta_core::process::process_start::process_start(Pid::current())
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(fixed_now);
    std::fs::write(lock_file(&repo), owner_record(std::process::id(), started)).unwrap();

    let err = prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, WorktreeError::Locked { .. }), "got: {err:?}");
}

#[tokio::test]
async fn a_legacy_empty_lock_refuses_conservatively_naming_the_file() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    std::fs::write(lock_file(&repo), "").unwrap();

    let err = prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("yunta-none.lock"),
        "the refusal must name the file to delete by hand: {message}"
    );
    assert!(
        !matches!(err, WorktreeError::Locked { .. }),
        "an unverifiable owner is its own case, not a live-owner refusal"
    );
}

// --- concurrent `git worktree` mutations never corrupt metadata -------

/// Git mutates `.git/worktrees/` without a complete lock between `add`s
/// — N concurrent additions on one repo can read each other's
/// half-written metadata (`failed to read .git/worktrees/<x>/commondir`).
/// Exactly what a `concurrency: N` task batch, two `kind: workflow`
/// nodes in one batch, or two MCP `run_workflow` calls do. The engine's
/// own lock around every worktree mutation is what makes this pass
/// deterministically.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_worktree_adds_on_one_repo_never_corrupt_git_metadata() {
    let owner = Owner::new();
    // `Supervision` is a copy of two borrows, so every task that spawns
    // in this round takes its own without moving what they point at.
    let supervision = owner.supervision();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    for round in 0..3 {
        let adds = (0..8).map(|i| {
            let repo = repo.clone();
            let base_commit = base_commit.clone();
            let path = root.path().join(format!("worktrees/r{round}-w{i}"));
            let branch = format!("yunta/stress/r{round}-w{i}");
            async move {
                prepare_worktree(
                    &repo,
                    &path,
                    &base_commit,
                    &branch,
                    Isolation::Worktree,
                    supervision,
                )
                .await
            }
        });
        let results = futures::future::join_all(adds).await;
        for (i, result) in results.into_iter().enumerate() {
            result.unwrap_or_else(|e| panic!("round {round}, add {i} failed: {e}"));
        }
    }
}

// --- one lock protocol, honest about what it cannot tell --------------

#[tokio::test]
async fn unknown_liveness_never_steals_a_lock() {
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("yunta.lock");
    let taken = fixed_now();
    let record = owner_record(4242, taken);
    tokio::fs::write(&lock_path, record).await.unwrap();
    let probe = Scripted {
        liveness: Liveness::Unknown,
        started: None,
    };

    // The `none` lock refuses at once, naming the holder it cannot verify.
    let err = acquire(
        &lock_path,
        Contention::Refuse,
        &probe,
        &FixedClock,
        &CancellationToken::new(),
        &take_over,
    )
    .await
    .unwrap_err();
    let LockError::Held {
        owner, liveness, ..
    } = err
    else {
        panic!("expected the lock to be held, got {err:?}");
    };
    assert_eq!(owner.pid.as_u32(), 4242);
    assert_eq!(liveness, Liveness::Unknown);

    // ...and the mutation lock waits out its patience, never stealing from it.
    let err = acquire(
        &lock_path,
        Contention::Wait {
            patience: Duration::from_millis(60),
            poll: Duration::from_millis(10),
        },
        &probe,
        &FixedClock,
        &CancellationToken::new(),
        &take_over,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LockError::Timeout { .. }), "got: {err:?}");
    assert_eq!(
        tokio::fs::read_to_string(&lock_path).await.unwrap(),
        owner_record(4242, taken),
        "the file still names the holder that could not be verified"
    );
}

#[tokio::test]
async fn a_reused_pid_is_not_the_owner() {
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("yunta.lock");
    let taken = fixed_now();
    std::fs::write(&lock_path, owner_record(4242, taken)).unwrap();

    // Alive, but started after the lock was taken: a newcomer that got
    // the old owner's pid.
    let newcomer = Scripted {
        liveness: Liveness::Alive,
        started: Some(taken + chrono::Duration::minutes(1)),
    };
    let acquired = acquire(
        &lock_path,
        Contention::Refuse,
        &newcomer,
        &FixedClock,
        &CancellationToken::new(),
        &take_over,
    )
    .await
    .unwrap();
    assert!(
        matches!(
            acquired,
            Acquired::Stolen { ref owner, ref dead }
                if dead.pid.as_u32() == 4242 && owner.pid == Pid::current()
        ),
        "got: {acquired:?}"
    );
    assert_eq!(read_owner(&lock_path).pid, Pid::current());
}

#[tokio::test]
async fn a_live_holder_with_an_unknown_start_time_keeps_its_lock() {
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("yunta.lock");
    std::fs::write(&lock_path, owner_record(4242, fixed_now())).unwrap();

    // A host that cannot tell when a process started: liveness alone
    // decides, and alive means held.
    let probe = Scripted {
        liveness: Liveness::Alive,
        started: None,
    };
    let err = acquire(
        &lock_path,
        Contention::Refuse,
        &probe,
        &FixedClock,
        &CancellationToken::new(),
        &take_over,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            err,
            LockError::Held {
                liveness: Liveness::Alive,
                ..
            }
        ),
        "got: {err:?}"
    );
}

/// A lock is released only while the file still names the owner releasing
/// it: a lock another process took is left in place.
#[test]
fn releasing_a_lock_another_process_took_leaves_it_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("yunta.lock");
    let taken = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
    std::fs::write(&lock_path, owner_record(4242, taken)).unwrap();
    let ours = LockOwner {
        schema_version: 1,
        pid: Pid::current(),
        started_at: taken,
    };

    release(&lock_path, &ours).unwrap();
    assert_eq!(
        std::fs::read_to_string(&lock_path).unwrap(),
        owner_record(4242, taken),
        "a lock another owner took is not this owner's to remove"
    );

    let theirs = read_owner(&lock_path);
    release(&lock_path, &theirs).unwrap();
    assert!(
        !lock_path.exists(),
        "the owner that names the lock removes it"
    );
}
