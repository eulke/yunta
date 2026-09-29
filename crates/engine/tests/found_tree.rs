//! What the run's tree holds that no node committed — a person's edits
//! while the run was parked — is committed as found when the next node
//! starts, so every attempt starts from a branch that holds it.

use yunta_core::events::{EventDraft, EventPayload, NodeEvent, NodeStartedPayload, StoredEvent};
use yunta_core::CommitSha;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git_output, write, Bench, MOCK_CONFIG};

mod common;
use common::{answer_parked, parked};

/// What each start of `node` found and committed, oldest first.
fn found(events: &[StoredEvent], node: &str) -> Vec<Option<CommitSha>> {
    events
        .iter()
        .filter(|event| event.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Started(p))) => Some(p.found.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn what_a_person_left_in_the_tree_during_a_pause_is_committed_as_found_when_the_next_node_starts(
) {
    let bench = parked(
        "name: fixed-by-hand\nnodes:\n  - { id: broken, kind: bash, run: \"test -f fixed.txt\" }\n",
        "sessions: []\n",
    )
    .await;
    write(&bench.worktree.join("fixed.txt"), "fixed by hand");

    answer_parked(&bench, "retry").await.unwrap();
    let RunReport { terminal, .. } = bench.wake_on_fixture("sessions: []\n").await;

    assert_eq!(terminal, RunTerminal::Finished);
    let starts = found(&bench.events(), "broken");
    assert!(
        matches!(starts[..], [None, Some(_)]),
        "the first attempt found nothing, the retry found the edit: {starts:?}"
    );
    assert_eq!(
        git_output(&bench.worktree, &["log", "-1", "--format=%s"]).trim(),
        "found in the run's tree before node broken started"
    );
    let files = git_output(&bench.worktree, &["ls-tree", "-r", "--name-only", "HEAD"]);
    assert!(files.contains("fixed.txt"), "{files}");
}

/// A crash mid-node leaves the attempt's work in the tree with no close
/// to commit it; the attempt that restarts the node commits it as found,
/// saying whose it was.
#[tokio::test]
async fn an_interrupted_attempts_leftovers_are_found_when_it_restarts() {
    let bench = Bench::new();
    bench
        .create(
            "name: restarts\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n",
            "sessions: []\n",
            MOCK_CONFIG,
        )
        .await;
    bench
        .storage
        .append(
            &EventDraft {
                run_id: bench.run_id.clone(),
                node_id: Some("only".into()),
                payload: EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
            },
            &yunta_testkit_core::FixedClock,
        )
        .unwrap();
    write(&bench.worktree.join("half.txt"), "half done");

    let RunReport { terminal, .. } = bench.wake().await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        found(&bench.events(), "only")[..],
        [None, Some(_)]
    ));
    let message = git_output(&bench.worktree, &["log", "-1", "--format=%B"]);
    assert!(
        message.starts_with("found in the run's tree before node only started")
            && message.contains("Holds what attempt 1 left when it was interrupted."),
        "{message}"
    );
}

/// A node with a checkout of its own opens it on everything the run's
/// tree holds; nothing is committed as found for it.
#[tokio::test]
async fn a_node_with_a_checkout_of_its_own_commits_nothing_found() {
    let bench = Bench::new();
    write(&bench.worktree.join("left.txt"), "left before the run");
    let workflow = r#"
name: scoped
nodes:
  - id: edit
    kind: bash
    scope: ["out.txt"]
    run: "test -f left.txt && echo out > out.txt"
"#;
    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []\n").await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(found(&bench.events(), "edit"), vec![None]);
}

/// A child starts while its group is running: what the tree holds then
/// may be its sibling's, so nothing is committed as found.
#[tokio::test]
async fn a_child_of_a_running_group_commits_nothing_found() {
    let bench = Bench::new();
    write(&bench.worktree.join("left.txt"), "left before the run");
    let workflow = r#"
name: grouped
nodes:
  - id: group
    kind: parallel
    nodes:
      - { id: a, kind: bash, run: "true" }
      - { id: b, kind: bash, run: "true" }
"#;
    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []\n").await;

    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    assert!(
        matches!(found(&events, "group")[..], [Some(_)]),
        "the group found it"
    );
    assert_eq!(found(&events, "a"), vec![None]);
    assert_eq!(found(&events, "b"), vec![None]);
}
