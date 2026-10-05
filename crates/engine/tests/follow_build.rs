//! A run follows the build its last unit left: once a loop's work lands,
//! the next node that works in the run's tree runs in the checkout that
//! task built in — warm at the commit the run stands at — instead of
//! building that commit again from cold in the tree the run was born in.

use std::path::PathBuf;

use yunta_core::events::{EventPayload, RunEvent, StoredEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git, Bench, ScriptedInteraction};

mod common;
use common::answer_parked;

/// A plan of one task, a loop working it, and `after` — a bash node in
/// the run's tree once the task's work landed.
fn builds_then(after: &str) -> String {
    format!(
        r#"
name: follows
nodes:
  - id: plan
    kind: bash
    run: "printf 'tasks:\n  - id: T001\n    title: Build\n    scope: [a.txt]\n    criteria:\n      - cmd: test -f a.txt\n' > {{{{node.artifacts}}}}/tasks.yaml"
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    concurrency: 1
    prompt: "Implement your task."
{after}"#
    )
}

/// The task's session builds, and writes its work.
const BUILDS: &str = "\
capabilities: { run_tools: true }
sessions:
  - effects:
      - { path: target/marker, content: built }
      - { path: a.txt, content: a }
    outcome: { type: completed, summary: built }
";

fn ignore(bench: &Bench, ignored: &str) {
    std::fs::write(bench.worktree.join(".gitignore"), ignored).unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "ignore builds"]);
}

/// Every checkout a `run_moved` of `events` names.
fn moves(events: &[StoredEvent]) -> Vec<PathBuf> {
    events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Run(RunEvent::Moved(moved))) => Some(moved.checkout.clone()),
            _ => None,
        })
        .collect()
}

const TESTS_THE_BUILD: &str =
    "  - { id: lint, kind: bash, depends_on: [implement], run: \"test -f target/marker\" }\n";

#[tokio::test]
async fn a_run_moves_into_the_checkout_its_last_task_built_in() {
    let bench = Bench::new().pooled();
    ignore(&bench, "target/\n");

    let RunReport { terminal, .. } = bench.run(&builds_then(TESTS_THE_BUILD), BUILDS).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(moves(&bench.events()), vec![bench.tree()]);
    assert!(bench.tree().join("target/marker").exists());
}

#[tokio::test]
async fn a_moved_run_wakes_where_it_moved() {
    let bench = Bench::new().pooled();
    ignore(&bench, "target/\n");
    let gated = format!("{TESTS_THE_BUILD}  - {{ id: approve, kind: gate, depends_on: [lint], assignee: lead, options: [approve] }}\n");
    let RunReport { terminal, .. } = bench.run(&builds_then(&gated), BUILDS).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let moved = bench.tree();
    answer_parked(&bench, "approve").await.unwrap();

    let RunReport { terminal, state } = bench.wake().await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(moves(&bench.events()), vec![moved.clone()]);
    assert_eq!(state.run.checkout(), Some(moved.as_path()));
    assert!(
        !state.run.moved_on_waking(),
        "the wake found the run where it moved"
    );
}

/// A gate between the loop and the node that builds next leaves the
/// build where the task left it: the run follows it once it builds.
#[tokio::test]
async fn a_run_follows_its_tasks_build_past_a_gate() {
    let bench = Bench::new().pooled();
    ignore(&bench, "target/\n");
    let gate = "  - { id: approve, kind: gate, depends_on: [implement], assignee: lead, options: [approve] }\n";
    let lint = TESTS_THE_BUILD.replace("[implement]", "[approve]");
    let RunReport { terminal, .. } = bench
        .run(&builds_then(&format!("{gate}{lint}")), BUILDS)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    answer_parked(&bench, "approve").await.unwrap();

    let RunReport { terminal, .. } = bench.wake().await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(moves(&bench.events()), vec![bench.tree()]);
}

/// A tree warm from before its task's work landed holds a build of an
/// older commit, however large: the run follows its task's build of the
/// commit it stands at.
#[tokio::test]
async fn a_run_leaves_a_larger_build_of_an_older_commit_for_its_tasks_build() {
    let bench = Bench::new().pooled();
    ignore(&bench, "target/\n");
    let older = "mkdir -p target && head -c 262144 /dev/zero > target/older && printf 'tasks:";
    let workflow = builds_then(TESTS_THE_BUILD).replace("printf 'tasks:", older);

    let RunReport { terminal, .. } = bench.run(&workflow, BUILDS).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(moves(&bench.events()), vec![bench.tree()]);
}

/// A build the run's tree holds that the task's checkout does not — what
/// a node before the loop left — keeps the run where it is, however much
/// more the task built.
#[tokio::test]
async fn a_run_stays_where_its_tree_holds_a_build_the_task_lacks() {
    let bench = Bench::new().pooled();
    ignore(&bench, "target/\n.cache/\n");
    let workflow = builds_then(TESTS_THE_BUILD)
        .replace(
            "printf 'tasks:",
            "mkdir -p .cache && echo kept > .cache/x && printf 'tasks:",
        )
        .replace(
            "run: \"test -f target/marker\"",
            "run: \"test -f .cache/x\"",
        );
    let builds_more = BUILDS.replace(
        "content: built",
        &format!("content: {}", "b".repeat(1 << 16)),
    );

    let RunReport { terminal, .. } = bench.run(&workflow, &builds_more).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(moves(&bench.events()), Vec::<PathBuf>::new());
}

/// A child that moved and then promoted hands its successor the checkout
/// it moved into, where what its task built is.
#[tokio::test]
async fn a_promoted_run_hands_over_the_checkout_it_moved_to() {
    let child = builds_then(
        "  - { id: lint, kind: bash, depends_on: [implement], run: \"test -f fixed.txt\", on_failure: { goto: fix, max_reroutes: 0 } }\n  - { id: fix, kind: bash, run: \"true\" }\n  - { id: ship, kind: bash, run: \"test -f target/marker\" }\n",
    )
    .replace("name: follows\n", "name: follows\nmodes:\n  quick: { include: [plan, implement, lint, fix] }\n  full: { include: [ship] }\n");
    let parent = "name: parent\nnodes:\n  - { id: feat, kind: workflow, use: follows }\n";
    let bench = Bench::new().with_workflow("follows", &child);
    ignore(&bench, "target/\n");

    let promote = ScriptedInteraction::choose("promote");
    let RunReport { terminal, .. } = bench.run_with_interaction(parent, BUILDS, &promote).await;

    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "the successor ships where the build is"
    );
}
