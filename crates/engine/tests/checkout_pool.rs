//! The checkouts a run's units work in, taken from their project's pool.
//! A unit takes a free checkout, put back to the commit it starts from,
//! and what git ignores — a build's output — stays for the next unit. A
//! checkout on a unit's branch is that unit's, and work that is still
//! somebody's — committed and not landed, or uncommitted — is never handed
//! to another unit.

mod pool_world;

use pool_world::{canonical, delete, ignore_builds, write, World, RUN};
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
    world.land(&first).await;
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
        format!("yunta/unit/{RUN}/task/T002/1")
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

/// A unit that reopens finds its checkout by the branch it is on, though
/// its directory is the pool's and not its own.
#[tokio::test]
async fn a_unit_is_reopened_by_its_branch() {
    let world = World::new();
    let (unit, held) = world.open("T001").await;
    write(&unit.worktree.join("a.txt"), "work");
    drop(held);

    let reopened = yunta_engine::reopen_unit(
        &world.repo,
        &world.run,
        UnitId::Task("T001".into()),
        None,
        world.owner.supervision(),
    )
    .await
    .unwrap()
    .expect("the checkout it worked in");

    assert_eq!(canonical(&reopened.worktree), canonical(&unit.worktree));
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

/// A unit's branch goes as its work lands on the run's tree: a finished
/// run leaves its checkouts in the pool on no branch, and no branch of a
/// unit whose work landed — without being asked to clean anything up.
#[tokio::test]
async fn a_landed_units_branch_is_gone() {
    let bench = yunta_testkit::Bench::new();
    ignore_builds(&bench.worktree);

    let report = bench
        .run(SECOND_FINDS_THE_FIRST_S_BUILD, BUILDS_THEN_WRITES)
        .await;

    assert_eq!(report.terminal, yunta_engine::RunTerminal::Finished);
    let listing = git_output(&bench.worktree, &["worktree", "list", "--porcelain"]);
    assert!(!listing.contains("refs/heads/yunta/unit/"), "{listing}");
    let branches = git_output(
        &bench.worktree,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads/yunta/unit/",
        ],
    );
    assert_eq!(branches.trim(), "", "every unit's work landed");
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

/// A checkout that holds a build goes before an empty one nearer the
/// unit's start: only what changed is built again in it, where the empty
/// one builds everything, its dependencies first.
#[tokio::test]
async fn a_checkout_holding_a_build_goes_before_a_nearer_empty_one() {
    let world = World::new();
    let (warm, warm_held) = world.open("T001").await;
    let (cold, cold_held) = world.open("T002").await;
    write(&warm.worktree.join("target/debug/built"), "built");
    world.land(&warm).await;
    world.land(&cold).await;
    drop((warm_held, cold_held));
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
