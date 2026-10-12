//! The mutation lock every worktree mutation takes: two locks share one
//! protocol, and a run waiting on a lock a live process holds ends when its
//! token fires.

use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;
use yunta_core::{Isolation, Pid};
use yunta_engine::lock::{self, LockError, SystemProbe};
use yunta_engine::process::Supervision;
use yunta_engine::{prepare_worktree, WorktreeError};
use yunta_testkit::{init_repo, Owner};
use yunta_testkit_core::FixedClock;

use super::head;
use super::lock::{dead_pid, lock_file, mutation_lock_file, owner_record, read_owner};

#[tokio::test]
async fn both_locks_share_one_protocol() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    // The same stale record on both locks: a dead holder that took them
    // an hour ago.
    let stale = owner_record(dead_pid(), Utc::now() - chrono::Duration::hours(1));
    std::fs::write(lock_file(&repo), &stale).unwrap();
    std::fs::write(mutation_lock_file(&repo), &stale).unwrap();
    // A hard link to the holder's file: it keeps the stale record if the
    // steal replaces the file, and shows the new one if the steal wrote
    // over it.
    let witness = root.path().join("witness");
    std::fs::hard_link(lock_file(&repo), &witness).unwrap();

    // `isolation: none`: stolen by remove + create_new — a new file, a
    // fresh owner record — and reported.
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
    assert!(
        matches!(
            prepared,
            yunta_engine::WorktreePrepared::StoleStaleLock { .. }
        ),
        "got: {prepared:?}"
    );
    let holder = read_owner(&lock_file(&repo));
    assert_eq!(holder.pid, Pid::current());
    let expected_start = yunta_core::process::process_start::process_start(Pid::current())
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(|| owner.supervision().clock.now());
    assert_eq!(holder.started_at, expected_start);
    assert_eq!(
        std::fs::read_to_string(&witness).unwrap(),
        stale,
        "a steal replaces the file; it never writes over the holder's file"
    );

    // The worktree-mutation lock: the same dead holder is stolen the
    // same way, the mutation runs, and the lock is released after it.
    let worktree = root.path().join("wt");
    prepare_worktree(
        &repo,
        &worktree,
        &base_commit,
        "yunta/shared-protocol",
        Isolation::Worktree,
        owner.supervision(),
    )
    .await
    .unwrap();
    assert!(worktree.join(".gitkeep").exists());
    assert!(
        !mutation_lock_file(&repo).exists(),
        "the mutation lock is released once the mutation is done"
    );
}

/// A run waiting on the mutation lock a live process holds ends at once when
/// its token fires, rather than waiting out the lock's patience.
#[tokio::test]
async fn a_cancelled_run_does_not_wait_out_the_mutation_lock() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    tokio::fs::create_dir_all(&repo).await.unwrap();
    init_repo(&repo);
    let mut holder = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    lock::hand_over(
        &repo.join(".git/yunta-worktree.lock"),
        Pid::try_from(holder.id()).unwrap(),
        &SystemProbe,
        &FixedClock,
    )
    .unwrap();
    let cancel = CancellationToken::new();
    let firing = cancel.clone();
    yunta_testkit::after(Duration::from_millis(200), move || firing.cancel());
    let started = std::time::Instant::now();

    let err = prepare_worktree(
        &repo,
        &root.path().join("wt"),
        &head(&repo),
        "yunta/run/x",
        Isolation::Worktree,
        Supervision::outside_any_run(&cancel, &FixedClock),
    )
    .await
    .unwrap_err();
    holder.kill().unwrap();
    holder.wait().unwrap();

    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(
        matches!(err, WorktreeError::Lock(LockError::Cancelled { .. })),
        "got {err:?}"
    );
}

/// A checkout is removed through the repository it names, whichever project
/// asks: its directory and its registration both go, and the prune that
/// follows runs where the checkout no longer is.
#[tokio::test]
async fn a_checkout_is_removed_through_the_repository_it_names() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    tokio::fs::create_dir_all(&repo).await.unwrap();
    init_repo(&repo);
    let checkout = root.path().join("removed-checkout");
    let at = checkout.display().to_string();
    yunta_testkit::git(&repo, &["worktree", "add", "-q", "--detach", &at]);
    tokio::fs::write(checkout.join("draft.txt"), "uncommitted")
        .await
        .unwrap();

    yunta_engine::remove_checkout(&checkout, Owner::new().supervision())
        .await
        .unwrap();

    assert!(!checkout.exists(), "the checkout's directory went");
    let listed = yunta_testkit::git_output(&repo, &["worktree", "list", "--porcelain"]);
    assert!(
        !listed.contains("removed-checkout"),
        "its registration went: {listed}"
    );
}
