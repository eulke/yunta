//! A run whose tree changes after an invariant verified it runs that
//! invariant again before it goes on — and stops there when the changed
//! tree no longer passes.

use yunta_core::events::{EventPayload, GateEvent, NodeEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

/// How many attempts of `node` the run started.
fn attempts(bench: &Bench, node: &str) -> usize {
    bench
        .events()
        .iter()
        .filter(|event| event.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Node(NodeEvent::Started(_)))
            )
        })
        .count()
}

/// A check of the tree, then a node after it running `after`, then a
/// gate a person would be asked.
fn checked_then(after: &str) -> String {
    format!(
        r#"
name: checked
nodes:
  - id: check
    kind: bash
    invariant: true
    run: "test ! -f broken.txt"
  - id: after
    kind: bash
    depends_on: [check]
    run: "{after}"
  - id: ship
    kind: gate
    assignee: lead
    message: "Ship?"
    depends_on: [after]
"#
    )
}

#[tokio::test]
async fn an_invariant_reverifies_the_tree_a_later_node_changed_before_the_gate() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run(&checked_then("echo more > more.txt"), "sessions: []\n")
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "parked at the gate, with nobody to ask: {terminal:?}"
    );
    assert_eq!(
        attempts(&bench, "check"),
        2,
        "once, and once for the new tree"
    );
}

#[tokio::test]
async fn an_invariant_that_fails_on_the_changed_tree_stops_the_run_before_the_gate() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run(&checked_then("touch broken.txt"), "sessions: []\n")
        .await;

    let RunTerminal::Paused { reason } = terminal else {
        panic!("the run stops on the check: {terminal:?}");
    };
    assert!(reason.starts_with("node `check` failed"), "{reason}");
    assert!(
        !bench.events().iter().any(|event| matches!(
            event.payload(),
            Some(EventPayload::Gates(GateEvent::Waiting(_)))
                if event.node_id.as_ref().is_some_and(|id| id.as_str() == "ship")
        )),
        "nobody is asked to ship a tree that fails its check"
    );
}

#[tokio::test]
async fn a_node_that_changes_nothing_leaves_every_invariant_standing() {
    let bench = Bench::new();
    bench.run(&checked_then("true"), "sessions: []\n").await;

    assert_eq!(attempts(&bench, "check"), 1);
}

/// `check` and `writer` run side by side: they meet outside the tree,
/// `writer` changes the tree and says so, and only then does `check`
/// read it — so whatever order their closes land in, `check`'s pass was
/// taken on a tree that moved while it ran.
const SIDE_BY_SIDE: &str = r#"
name: side-by-side
nodes:
  - id: check
    kind: bash
    invariant: true
    run: "mkdir -p '{{run.dir}}/meet' && touch '{{run.dir}}/meet/check' && while [ ! -f '{{run.dir}}/meet/written' ]; do sleep 0.05; done; test ! -f broken.txt"
  - id: writer
    kind: bash
    run: "mkdir -p '{{run.dir}}/meet' && touch '{{run.dir}}/meet/writer' && while [ ! -f '{{run.dir}}/meet/check' ]; do sleep 0.05; done; echo more > more.txt && touch '{{run.dir}}/meet/written'"
  - id: after
    kind: bash
    depends_on: [check, writer]
    run: "true"
"#;

#[tokio::test]
async fn an_invariant_run_beside_a_writer_runs_again_alone() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(
            SIDE_BY_SIDE,
            "sessions: []\n",
            "defaults:\n  max_parallel_nodes: 2\n",
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        attempts(&bench, "check"),
        2,
        "once beside the writer, once alone on the tree it left"
    );
}
