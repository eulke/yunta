//! A failure sent back to an earlier node re-runs, in order, every node
//! between that node and the one that failed — a gate among them asks
//! again, about what was made again, before the failed node retries.

mod common;

use common::SequencedInteraction;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

/// `build`, a gate that approves it, and a `ship` that fails once and
/// sends the run back to `build`.
const WORKFLOW: &str = r#"
name: gated-reroute
nodes:
  - id: build
    kind: bash
    run: "echo built >> builds.txt"
  - id: approve
    kind: gate
    depends_on: [build]
    assignee: lead
    message: "Approve the build?"
  - id: ship
    kind: bash
    depends_on: [approve]
    run: "test -e shipped-once || { echo x > shipped-once; exit 1; }"
    on_failure: { goto: build, max_reroutes: 1 }
"#;

#[tokio::test]
async fn a_gate_between_a_correction_and_the_node_that_failed_asks_again_before_that_node_retries()
{
    let interaction = SequencedInteraction::choosing(&["approve", "approve"]);
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(WORKFLOW, "sessions: []\n", &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        interaction.shown().len(),
        2,
        "the gate asks about the build made again"
    );
    let builds = tokio::fs::read_to_string(bench.worktree.join("builds.txt"))
        .await
        .unwrap();
    assert_eq!(builds.lines().count(), 2);
}
