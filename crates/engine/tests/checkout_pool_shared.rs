//! The project's checkouts across runs and processes. A checkout is held
//! by a lock file that lasts as long as its process: another run's live
//! hold is respected, a gone holder's is taken back, and a checkout a
//! crash left half added is made again. A second run finds the first
//! run's checkouts where they were, with their builds, and the pool keeps
//! no more free checkouts than its runs had busy at once.

mod pool_world;

use std::path::Path;
use std::time::Duration;

use pool_world::{write, World};
use yunta_core::{Pid, RunId};

/// Hands checkout 1's lock to the process `pid`, as if it held it.
fn lock_slot_one_for(home: &Path, pid: u32) {
    let lock = home.join("slot-1.lock");
    yunta_engine::lock::hand_over(
        &lock,
        Pid::try_from(pid).unwrap(),
        &yunta_engine::lock::SystemProbe,
        &yunta_testkit_core::FixedClock,
    )
    .unwrap();
}

/// A checkout another live process holds is never handed out.
#[tokio::test]
async fn a_slot_locked_by_a_live_foreign_pid_is_skipped() {
    let world = World::new();
    let (first, held) = world.open("T001").await;
    world.land(&first, &held).await;
    drop(held);
    let home = world.pool.home(world.owner.supervision()).await.unwrap();
    let mut alive = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    lock_slot_one_for(&home, alive.id());

    let (second, _held) = world.open("T002").await;

    alive.kill().unwrap();
    alive.wait().unwrap();
    assert_ne!(second.worktree, first.worktree);
    assert_eq!(world.slots().await, vec!["slot-1", "slot-2"]);
}

/// A checkout whose holder is gone is taken back.
#[tokio::test]
async fn a_slot_whose_holder_died_is_taken_back() {
    let world = World::new();
    let (first, held) = world.open("T001").await;
    world.land(&first, &held).await;
    drop(held);
    let home = world.pool.home(world.owner.supervision()).await.unwrap();
    let mut gone = std::process::Command::new("true").spawn().unwrap();
    gone.wait().unwrap();
    lock_slot_one_for(&home, gone.id());

    let (second, _held) = world.open("T002").await;

    assert_eq!(second.worktree, first.worktree);
    assert_eq!(world.slots().await, vec!["slot-1"]);
}

/// A checkout a crash left half added — a directory git does not know as
/// a working tree — is made again under the same number.
#[tokio::test]
async fn a_half_added_slot_is_rebuilt() {
    let world = World::new();
    let home = world.pool.home(world.owner.supervision()).await.unwrap();
    write(&home.join("slot-1/leftover"), "half added");

    let (unit, _held) = world.open("T001").await;

    let made_again = tokio::fs::canonicalize(home.join("slot-1")).await.unwrap();
    assert_eq!(unit.worktree, made_again);
    assert!(!unit.worktree.join("leftover").exists());
    assert_eq!(world.slots().await, vec!["slot-1"]);
}

/// A second run's unit takes the checkout the first run's unit worked in,
/// at its path, and finds the build that unit left there.
#[tokio::test]
async fn a_second_runs_unit_reuses_the_first_runs_checkout_at_its_path() {
    let world = World::new();
    let (first, held) = world.open("T001").await;
    write(&first.worktree.join("target/marker"), "built");
    world.land(&first, &held).await;
    drop(held);
    let later = RunId::from("01JPOOLEDCHECKOUTSLATER00A");
    let their_pool = World::pool_of(world.root.path(), &world.repo, "later-run");

    let (second, _held) = world.open_through(&their_pool, &later, "T001", 1).await;

    assert_eq!(second.worktree, first.worktree);
    assert!(second.worktree.join("target/marker").exists());
}

/// A checkout holding a unit's work that has not landed is that unit's:
/// nobody else is handed it, and the unit's next attempt takes it back.
#[tokio::test]
async fn a_checkout_holding_unlanded_work_goes_back_to_its_unit_only() {
    let world = World::new();
    let (first, held) = world.open("T001").await;
    yunta_testkit::git(
        &first.worktree,
        &["commit", "-q", "--allow-empty", "-m", "left"],
    );
    drop(held);

    let (other, held_by_other) = world.open("T002").await;
    let (again, _held) = world.open_through(&world.pool, &world.run, "T001", 2).await;

    drop(held_by_other);
    assert_ne!(other.worktree, first.worktree);
    assert_eq!(again.worktree, first.worktree);
}

/// A unit whose work landed keeps its branch until somebody takes its
/// checkout: the next unit may, and finds it the nearest.
#[tokio::test]
async fn a_checkout_whose_work_landed_is_free_with_its_branch() {
    let world = World::new();
    let (first, held) = world.open("T001").await;
    drop(held);

    let (next, _held) = world.open("T002").await;

    assert_eq!(next.worktree, first.worktree);
    assert_eq!(world.slots().await, vec!["slot-1"]);
}

/// Past its budget, the pool empties the build of the free checkout used
/// least recently — never the one about to be taken — and keeps its
/// source.
#[tokio::test]
async fn past_its_budget_the_pool_empties_the_least_recently_used_free_build() {
    let world = World::new();
    let (older, older_held) = world.open("T001").await;
    let (newer, newer_held) = world.open("T002").await;
    for unit in [&older, &newer] {
        write(&unit.worktree.join("target/build"), &"b".repeat(64 * 1024));
    }
    world.land(&older, &older_held).await;
    world.land(&newer, &newer_held).await;
    world
        .give_back_at(&older, older_held, "2026-01-01T00:00:00Z")
        .await;
    world
        .give_back_at(&newer, newer_held, "2026-01-02T00:00:00Z")
        .await;
    let budget = yunta_core::PoolConfig {
        budget: Some(yunta_core::DiskShare::Bytes(100_000)),
        keep_free: Some(yunta_core::DiskShare::Bytes(0)),
    };
    let pool = yunta_engine::CheckoutPool::within(
        &world.root.path().join("worktrees"),
        &world.repo,
        &world.root.path().join("run"),
        &budget,
    );

    let (taken, _held) = world.open_through(&pool, &world.run, "T003", 1).await;

    assert_eq!(taken.worktree, newer.worktree);
    assert!(
        !older.worktree.join("target/build").exists(),
        "the older build went"
    );
    assert!(
        older.worktree.join(".gitignore").exists(),
        "its source stayed"
    );
}

/// A checkout that would leave less of the disk free than the pool keeps
/// free is refused, saying so.
#[tokio::test]
async fn a_checkout_is_refused_when_it_would_leave_less_than_kept_free() {
    let world = World::new();
    let everything = yunta_core::PoolConfig {
        budget: None,
        keep_free: Some(yunta_core::DiskShare::Percent(100)),
    };
    let pool = yunta_engine::CheckoutPool::within(
        &world.root.path().join("worktrees"),
        &world.repo,
        &world.root.path().join("run"),
        &everything,
    );

    let refused = pool
        .open_detached(&world.head(), world.owner.supervision())
        .await;

    assert!(
        matches!(refused, Err(yunta_engine::WorktreeError::NoRoom { .. })),
        "{:?}",
        refused.map(|(path, _)| path)
    );
}

/// A run's own checkout is the run's while it lives, parked or not: no
/// unit of another run and no other run is handed it, though it holds
/// nothing but the commit it opened on.
#[tokio::test]
async fn a_paused_runs_checkout_is_never_leased_to_another_run() {
    let world = World::new();
    let parked = RunId::from("01JPOOLPARKEDRUNCHECKOUT0A");
    let supervision = world.owner.supervision();
    let (theirs, held) = world
        .pool
        .open_run(&parked, &world.head(), supervision)
        .await
        .unwrap();
    drop(held);

    let (unit, _held) = world.open("T001").await;
    let other = RunId::from("01JPOOLANOTHERRUNCHECKOUTA");
    let (others, _held) = world
        .pool
        .open_run(&other, &world.head(), supervision)
        .await
        .unwrap();

    assert_ne!(unit.worktree, theirs);
    assert_ne!(others, theirs);
}

/// A checkout a person switched to a branch of their own while the run was
/// parked is still the run's; one on another run's branch is not.
#[tokio::test]
async fn resume_continues_on_a_persons_branch_in_the_bound_checkout() {
    let world = World::new();
    let run = RunId::from("01JPOOLPERSONSBRANCHRUN00A");
    let supervision = world.owner.supervision();
    let (tree, _held) = world
        .pool
        .open_run(&run, &world.head(), supervision)
        .await
        .unwrap();

    yunta_testkit::git(&tree, &["switch", "-q", "-c", "my-fix"]);
    let a_persons = yunta_engine::still_the_runs(&tree, &run, supervision).await;
    yunta_testkit::git(
        &tree,
        &[
            "switch",
            "-q",
            "-c",
            "yunta/run/01JPOOLSOMEBODYELSE0000000A",
        ],
    );
    let another_runs = yunta_engine::still_the_runs(&tree, &run, supervision).await;

    assert!(a_persons);
    assert!(!another_runs);
}

/// Putting a free checkout on a unit's branch reads every checkout of the
/// repository, so it waits for a checkout another process is adding
/// beside it, whose metadata may be half written.
#[tokio::test]
async fn a_reused_checkout_waits_for_a_worktree_being_added_beside_it() {
    let world = World::new();
    let (first, held) = world.open("T001").await;
    world.land(&first, &held).await;
    drop(held);
    let common = yunta_testkit::git_output(&world.repo, &["rev-parse", "--git-common-dir"]);
    let lock = world.repo.join(common.trim()).join("yunta-worktree.lock");
    let mut adding = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let holder = Pid::try_from(adding.id()).unwrap();
    let probe = yunta_engine::lock::SystemProbe;
    yunta_engine::lock::hand_over(&lock, holder, &probe, &yunta_testkit_core::FixedClock).unwrap();

    let waited = tokio::time::timeout(Duration::from_millis(500), world.open("T002")).await;
    adding.kill().unwrap();
    adding.wait().unwrap();

    assert!(
        waited.is_err(),
        "the reuse went ahead while a checkout was being added"
    );
    let (second, _held) = world.open("T002").await;
    assert_eq!(second.worktree, first.worktree);
}
