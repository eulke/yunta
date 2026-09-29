//! A node's finish names the run's tree as the node left it: what a check
//! that verified the tree is later measured against.

use yunta_core::events::{EventPayload, NodeEvent};
use yunta_core::TreeId;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{Bench, MOCK_CONFIG};

mod common;
use common::SequencedInteraction;

/// The tree each finish of `node` names, oldest first.
fn left(bench: &Bench, node: &str) -> Vec<Option<TreeId>> {
    bench
        .events()
        .iter()
        .filter(|event| event.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Finished(p))) => Some(p.tree.clone()),
            _ => None,
        })
        .collect()
}

/// The tree the bench's worktree holds now, captured the way the engine
/// captures one, through an index of the test's own.
async fn tree_now(bench: &Bench) -> TreeId {
    let index = tempfile::tempdir().unwrap();
    let owner = yunta_testkit::Owner::new();
    yunta_engine::capture_tree(
        &bench.worktree,
        &index.path().join("index"),
        owner.supervision(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn a_node_finish_names_the_tree_it_left_the_run_at() {
    let bench = Bench::new();
    let workflow = r#"
name: leaves-a-file
nodes:
  - id: write
    kind: bash
    run: "echo made > made.txt"
  - id: land
    kind: bash
    depends_on: [write]
    scope: [landed.txt]
    run: "echo landed > landed.txt"
"#;
    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []\n").await;
    assert_eq!(terminal, RunTerminal::Finished);

    let now = tree_now(&bench).await;
    let written = left(&bench, "write");
    let [Some(written)] = &written[..] else {
        panic!("one finish, naming a tree: {written:?}");
    };
    assert_ne!(
        written, &now,
        "`land` changed the tree after `write` left it"
    );
    assert_eq!(
        left(&bench, "land"),
        vec![Some(now)],
        "a node with a checkout of its own names the run's tree it landed in"
    );
}

#[tokio::test]
async fn a_gate_finish_names_the_tree_its_decision_saw() {
    let bench = Bench::new();
    let workflow = r#"
name: decided
nodes:
  - id: write
    kind: bash
    run: "echo made > made.txt"
  - id: approve
    kind: gate
    depends_on: [write]
    assignee: lead
    message: "Go on?"
    options: [yes]
"#;
    let RunReport { terminal, .. } = bench
        .run_full(
            workflow,
            "sessions: []\n",
            MOCK_CONFIG,
            &SequencedInteraction::choosing(&["yes"]),
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(left(&bench, "approve"), vec![Some(tree_now(&bench).await)]);
}

/// A failure can move the tree as much as a finish can, so it names the
/// tree it left too.
#[tokio::test]
async fn a_node_failure_names_the_tree_it_left_the_run_at() {
    let bench = Bench::new();
    let workflow = "name: fails\nnodes:\n  - { id: broke, kind: bash, run: \"echo half > half.txt; exit 1\" }\n";
    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []\n").await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let failed: Vec<Option<TreeId>> = bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(p))) => Some(p.tree.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(failed, vec![Some(tree_now(&bench).await)]);
}
