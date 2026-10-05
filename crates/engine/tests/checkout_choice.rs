//! Which checkout of the pool a user takes, by what the pool knows of each
//! checkout's build: a user that builds takes the warmest, a run whose
//! units build takes the emptiest, and a build the pool never recorded or
//! has not counted is learned rather than guessed.

mod pool_world;

use pool_world::{canonical, delete, write, World};

/// A run whose units build its work is born in the free checkout holding
/// the least, and leaves the warm one to its units; a run that builds in
/// its own tree is born in the warm one.
#[tokio::test]
async fn a_run_whose_units_build_is_born_in_the_emptiest_free_checkout() {
    let world = World::new();
    let (warm, warm_held) = world.open("T001").await;
    let (empty, empty_held) = world.open("T002").await;
    write(&warm.worktree.join("target/build"), &"b".repeat(64 * 1024));
    for (unit, held) in [(&warm, warm_held), (&empty, empty_held)] {
        world.land(unit, &held).await;
        world.give_back(unit, held).await;
    }

    let (following, _held) = born(&world, yunta_engine::Birth::FollowsUnits).await;
    let (building, _also) = born(&world, yunta_engine::Birth::Builds).await;

    assert_eq!(canonical(&following), canonical(&empty.worktree));
    assert_eq!(canonical(&building), canonical(&warm.worktree));
}

/// What nobody has counted is not known to be light: a run whose units
/// build takes a small build that was counted before one whose count is
/// still under way.
#[tokio::test]
async fn a_build_nobody_has_counted_is_not_taken_for_a_light_one() {
    let world = World::new();
    let (counted, counted_held) = world.open("T001").await;
    let (pending, pending_held) = world.open("T002").await;
    for (unit, held) in [(&counted, counted_held), (&pending, pending_held)] {
        write(&unit.worktree.join("target/build"), "b");
        world.land(unit, &held).await;
        world.give_back(unit, held).await;
    }
    let name = pending.worktree.file_name().unwrap().to_string_lossy();
    let target = pending.worktree.join("target");
    let uncounted = format!(
        r#"{{"roots":["{}"],"at":"2026-01-01T00:00:00Z"}}"#,
        target.display()
    );
    let record = pending.worktree.with_file_name(format!("{name}.built"));
    tokio::fs::write(record, uncounted).await.unwrap();

    let (following, _held) = born(&world, yunta_engine::Birth::FollowsUnits).await;

    assert_eq!(canonical(&following), canonical(&counted.worktree));
}

/// A run whose units build lets the pool keep its budget before it
/// chooses, and takes the checkout whose build the budget just emptied.
#[tokio::test]
async fn a_run_whose_units_build_takes_a_checkout_the_budget_just_emptied() {
    let world = World::new();
    let (older, older_held) = world.open("T001").await;
    let (newer, newer_held) = world.open("T002").await;
    for (unit, held, at) in [
        (&older, older_held, "2026-01-01T00:00:00Z"),
        (&newer, newer_held, "2026-01-02T00:00:00Z"),
    ] {
        write(&unit.worktree.join("target/build"), &"b".repeat(64 * 1024));
        world.land(unit, &held).await;
        world.give_back_at(unit, held, at).await;
    }
    let budget = yunta_core::PoolConfig {
        budget: Some(yunta_core::DiskShare::Bytes(100_000)),
        keep_free: Some(yunta_core::DiskShare::Bytes(0)),
    };
    let root = world.root.path();
    let pool = yunta_engine::CheckoutPool::within(
        &root.join("worktrees"),
        &world.repo,
        &root.join("run"),
        &budget,
    );
    let run = yunta_core::RunId::from("01JBORNWITHINITSBUDGET");
    let birth = yunta_engine::Birth::FollowsUnits;

    let (tree, _held) = pool
        .open_run(&run, &world.head(), birth, world.owner.supervision())
        .await
        .unwrap();

    assert_eq!(canonical(&tree), canonical(&older.worktree));
    assert!(
        newer.worktree.join("target/build").exists(),
        "the newer build stays"
    );
}

/// A run of its own opened in the pool, born as `birth` says.
async fn born(
    world: &World,
    birth: yunta_engine::Birth,
) -> (std::path::PathBuf, yunta_engine::Lease) {
    let run = yunta_core::RunId::from(format!("01JBORN{birth:?}").as_str());
    let supervision = world.owner.supervision();
    world
        .pool
        .open_run(&run, &world.head(), birth, supervision)
        .await
        .unwrap()
}

/// A checkout kept before builds were recorded is known by what git
/// ignores in it at the next take: the warm one goes to a user that
/// builds, however long ago it was used, and the empty one to a run whose
/// units build.
#[tokio::test]
async fn a_checkout_nobody_recorded_is_known_by_what_it_holds() {
    let world = World::new();
    let (warm, warm_held) = world.open("T001").await;
    let (empty, empty_held) = world.open("T002").await;
    write(&warm.worktree.join("target/build"), &"b".repeat(64 * 1024));
    world.land(&warm, &warm_held).await;
    world.land(&empty, &empty_held).await;
    world
        .give_back_at(&warm, warm_held, "2026-01-01T00:00:00Z")
        .await;
    world
        .give_back_at(&empty, empty_held, "2026-01-02T00:00:00Z")
        .await;
    for unit in [&warm, &empty] {
        let name = unit.worktree.file_name().unwrap().to_string_lossy();
        delete(&unit.worktree.with_file_name(format!("{name}.built")));
    }
    let supervision = world.owner.supervision();

    let (measured, _held) = world
        .pool
        .open_detached(&world.head(), supervision)
        .await
        .unwrap();
    let (following, _also) = born(&world, yunta_engine::Birth::FollowsUnits).await;

    assert_eq!(canonical(&measured), canonical(&warm.worktree));
    assert_eq!(canonical(&following), canonical(&empty.worktree));
}

/// A checkout the pool adds holds its source and no build, and says so.
#[tokio::test]
async fn a_checkout_the_pool_adds_is_recorded_without_a_build() {
    let world = World::new();

    let (unit, _held) = world.open("T001").await;

    let name = unit.worktree.file_name().unwrap().to_string_lossy();
    let record = std::fs::read_to_string(unit.worktree.with_file_name(format!("{name}.built")));
    assert!(record.unwrap().contains("\"bytes\":0"));
}
