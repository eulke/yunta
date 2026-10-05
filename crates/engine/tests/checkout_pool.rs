//! The checkouts a run's units work in, taken from their project's pool.
//! A unit takes a free checkout, put back to the commit it starts from,
//! and what git ignores — a build's output — stays for the next unit. A
//! checkout on a unit's branch is that unit's, and work that is still
//! somebody's — committed and not landed, or uncommitted — is never handed
//! to another unit.

mod pool_world;

use pool_world::{canonical, delete, ignore_builds, write, World};
use yunta_engine::UnitId;
use yunta_testkit::{git, git_output};

/// A unit whose work landed leaves its checkout to the next, which finds
/// what git ignores where the first left it and nothing else of its work.
#[tokio::test]
async fn a_landed_units_checkout_is_reused_with_its_ignored_files_and_nothing_else() {
    let world = World::new();
    let (first, held) = world.open("T001").await;
    write(&first.worktree.join("target/marker"), "built");
    write(
        &first
            .worktree
            .join(yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE),
        "asked",
    );
    world.land(&first, &held).await;
    drop(held);

    let (second, _held) = world.open("T002").await;

    assert_eq!(second.worktree, first.worktree);
    assert!(second.worktree.join("target/marker").exists());
    assert!(!second
        .worktree
        .join(yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE)
        .exists());
    assert_eq!(
        git_output(&second.worktree, &["branch", "--show-current"]).trim(),
        "",
        "a unit works on no branch"
    );
    assert_eq!(world.slots().await, vec!["slot-1"]);
}

/// Two units at work at once never share a checkout.
#[tokio::test]
async fn a_held_checkout_is_never_given_to_another_unit() {
    let world = World::new();
    let (first, _held) = world.open("T001").await;

    let (second, _also) = world.open("T002").await;

    assert_ne!(second.worktree, first.worktree);
    assert_eq!(world.slots().await, vec!["slot-1", "slot-2"]);
}

/// Work that is still somebody's keeps its checkout: committed and not
/// landed — a blocked task's — or uncommitted.
#[tokio::test]
async fn a_checkout_holding_work_not_landed_stays_out_of_the_pool() {
    let world = World::new();
    let (blocked, held) = world.open("T001").await;
    write(&blocked.worktree.join("a.txt"), "work");
    git(&blocked.worktree, &["add", "-A"]);
    git(&blocked.worktree, &["commit", "-q", "-m", "left work"]);
    drop(held);
    let (dirty, held) = world.open("T002").await;
    write(&dirty.worktree.join("b.txt"), "unsaved");
    drop(held);

    let (third, _held) = world.open("T003").await;

    assert_ne!(third.worktree, blocked.worktree);
    assert_ne!(third.worktree, dirty.worktree);
    assert_eq!(world.slots().await, vec!["slot-1", "slot-2", "slot-3"]);
}

/// A blocked task's checkout is reopened by its record: the session
/// picked back up works on where it saw its work, though the checkout is
/// the pool's and on no branch.
#[tokio::test]
async fn a_blocked_tasks_checkout_is_reopened_by_its_record() {
    let world = World::new();
    let (unit, held) = world.open("T001").await;
    write(&unit.worktree.join("a.txt"), "work");
    yunta_engine::commit_work(&unit, "left work", world.owner.supervision())
        .await
        .unwrap();
    let left = world.head_of(&unit.worktree);
    world.give_back(&unit, held).await;
    let (_other, _held) = world.open("T002").await;

    let (reopened, _held) = world
        .pool
        .reopen_unit(
            &world.run,
            UnitId::Task("T001".into()),
            Some(&left),
            world.owner.supervision(),
        )
        .await
        .unwrap()
        .expect("the checkout it worked in");

    assert_eq!(canonical(&reopened.worktree), canonical(&unit.worktree));
    assert!(
        reopened.worktree.join("a.txt").exists(),
        "its work is back, uncommitted"
    );
}

/// Two tasks worked one after the other: the second's criterion passes
/// only where the first one's build output still is, which a checkout of
/// its own would not have.
const SECOND_FINDS_THE_FIRST_S_BUILD: &str = r#"
name: warm-builds
nodes:
  - id: plan
    kind: bash
    run: "printf 'tasks:\n  - id: T001\n    title: First\n    scope: [a.txt]\n    criteria:\n      - cmd: test -f a.txt\n  - id: T002\n    title: Second\n    scope: [b.txt]\n    depends_on: [T001]\n    criteria:\n      - cmd: test -f b.txt && test -f target/marker\n' > {{node.artifacts}}/tasks.yaml"
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    concurrency: 1
    prompt: "Implement your task."
"#;

const BUILDS_THEN_WRITES: &str = "\
capabilities: { run_tools: true }
sessions:
  - effects:
      - { path: target/marker, content: built }
      - { path: a.txt, content: a }
    outcome: { type: completed, summary: built }
  - effects:
      - { path: b.txt, content: b }
    outcome: { type: completed, summary: wrote }
";

#[tokio::test]
async fn a_loop_s_tasks_build_in_one_warm_checkout() {
    let bench = yunta_testkit::Bench::new();
    ignore_builds(&bench.worktree);

    let report = bench
        .run(SECOND_FINDS_THE_FIRST_S_BUILD, BUILDS_THEN_WRITES)
        .await;

    assert_eq!(report.terminal, yunta_engine::RunTerminal::Finished);
    assert_eq!(bench.checkouts(), vec!["slot-1"]);
}

/// A finished run leaves no branch of a unit, and no ref of one whose
/// work landed — without being asked to clean anything up.
#[tokio::test]
async fn a_finished_runs_units_leave_no_branch() {
    let bench = yunta_testkit::Bench::new();
    ignore_builds(&bench.worktree);

    let report = bench
        .run(SECOND_FINDS_THE_FIRST_S_BUILD, BUILDS_THEN_WRITES)
        .await;

    assert_eq!(report.terminal, yunta_engine::RunTerminal::Finished);
    let units = git_output(
        &bench.worktree,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads/yunta/unit/",
            "refs/yunta/unit/",
        ],
    );
    assert_eq!(units.trim(), "", "every unit's work landed");
}

/// A private index that starts as a copy of the checkout's own captures
/// exactly the tree one built from nothing does: what is untracked, what
/// changed and what is gone are all in it.
#[tokio::test]
async fn a_seeded_private_index_captures_what_a_fresh_one_does() {
    let world = World::new();
    write(&world.repo.join("kept.txt"), "kept");
    write(&world.repo.join("gone.txt"), "gone");
    git(&world.repo, &["add", "-A"]);
    git(&world.repo, &["commit", "-q", "-m", "files"]);
    write(&world.repo.join("kept.txt"), "changed");
    delete(&world.repo.join("gone.txt"));
    write(&world.repo.join("new.txt"), "new");
    let scratch = tempfile::tempdir().unwrap();
    let fresh = scratch.path().join("fresh-index");
    let env = vec![("GIT_INDEX_FILE".to_string(), fresh.display().to_string())];
    let from_nothing = world.owner.supervision().with_env(&env);
    yunta_engine::git::output(&world.repo, &["add", "-A"], from_nothing)
        .await
        .unwrap();
    let expected = yunta_engine::git::output(&world.repo, &["write-tree"], from_nothing)
        .await
        .unwrap();

    let captured = yunta_engine::capture_tree(
        &world.repo,
        &scratch.path().join("seeded-index"),
        world.owner.supervision(),
    )
    .await
    .unwrap();

    assert_eq!(captured.as_str(), expected.trim());
}

/// A checkout given back holding a build goes before an empty one nearer
/// the unit's start: the pool knows the build by the record the checkout
/// was given back with, and only what changed is built again in it, where
/// the empty one builds everything, its dependencies first.
#[tokio::test]
async fn a_build_is_known_by_the_record_its_checkout_was_given_back_with() {
    let world = World::new();
    let (warm, warm_held) = world.open("T001").await;
    let (cold, cold_held) = world.open("T002").await;
    write(&warm.worktree.join("target/debug/built"), "built");
    world.land(&warm, &warm_held).await;
    world.land(&cold, &cold_held).await;
    world.give_back(&warm, warm_held).await;
    world.give_back(&cold, cold_held).await;
    write(&world.repo.join("moved.txt"), "moved");
    yunta_testkit::git(&world.repo, &["add", "-A"]);
    yunta_testkit::git(&world.repo, &["commit", "-q", "-m", "moved"]);
    let start = world.head();
    yunta_testkit::git(
        &cold.worktree,
        &["switch", "-q", "--detach", start.as_str()],
    );

    let (next, _held) = world.open("T003").await;

    assert_eq!(next.worktree, warm.worktree);
}

/// Opening a unit asks git as often whether the pool holds two free
/// checkouts or eight: one listing answers for all of them, and only the
/// checkout taken is looked at on its own.
#[tokio::test]
async fn opening_a_unit_asks_git_as_often_whatever_the_pool_holds() {
    let few = asked_to_open_beside(2).await;
    let many = asked_to_open_beside(8).await;

    assert_eq!(few, many);
    assert!(many <= 6, "opening a unit ran git {many} times");
}

/// How many times opening one unit runs git, in a pool holding `free`
/// checkouts given back after their units landed.
async fn asked_to_open_beside(free: usize) -> usize {
    let world = World::new();
    let mut opened = Vec::new();
    for n in 0..free {
        opened.push(world.open(&format!("T{n:03}")).await);
    }
    for (unit, lease) in opened {
        world.land(&unit, &lease).await;
        world.give_back(&unit, lease).await;
    }
    let log = world.root.path().join("git.log");
    let stub = yunta_testkit::stubs::git_counting(world.root.path(), &log);
    let supervision = yunta_engine::process::Supervision {
        env: &stub,
        ..world.owner.supervision()
    };

    world.open_supervised("T999", supervision).await;

    let asked = tokio::fs::read_to_string(&log).await.unwrap_or_default();
    asked.lines().count()
}

/// As its run ends, a blocked task's clean checkout goes back to the pool
/// for anybody, while the ref that keeps its work stays until the run is
/// collected.
#[tokio::test]
async fn run_end_frees_a_blocked_units_clean_checkout_and_keeps_its_ref() {
    let world = World::new();
    let (blocked, held) = world.open("T001").await;
    write(&blocked.worktree.join("a.txt"), "left");
    yunta_engine::commit_work(&blocked, "left work", world.owner.supervision())
        .await
        .unwrap();
    world.give_back(&blocked, held).await;
    let run_dir = world.root.path().join("run");

    yunta_engine::release_unit_checkouts(
        &world.pool,
        &world.repo,
        &run_dir,
        &world.run,
        world.owner.supervision(),
    )
    .await
    .unwrap();

    let other = World::pool_of(world.root.path(), &world.repo, "other");
    let (taken, _held) = world
        .open_through(
            &other,
            &yunta_core::RunId::from("01JOTHERRUN000000000000000"),
            "T001",
            1,
        )
        .await;
    assert_eq!(canonical(&taken.worktree), canonical(&blocked.worktree));
    let kept = git_output(
        &world.repo,
        &["for-each-ref", "--format=%(refname)", "refs/yunta/unit/"],
    );
    assert_eq!(kept.trim(), blocked.keeps.name());
}
