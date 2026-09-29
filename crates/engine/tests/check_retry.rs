//! A check that judges the run's tree answers the same on the same tree:
//! a `retry` of one whose tree nothing changed since it failed fails at
//! once, without running its command, and asks again.

use std::path::PathBuf;

use yunta_core::events::{EventPayload, Failure, NodeEvent};
use yunta_engine::{current_escalation, RunReport, RunTerminal};
use yunta_testkit::{write, Bench, MOCK_CONFIG};

mod common;
use common::answer_parked;

/// A node that breaks what the suite checks, then the comparison.
const REGRESSES: &str = r#"
name: compared
nodes:
  - id: regress
    kind: bash
    run: "rm marker.txt"
  - id: compare
    kind: check
    builtin: baseline_compare
    depends_on: [regress]
"#;

/// Where a command counts its own runs: outside the tree, so counting
/// never changes what it judges.
fn counter(bench: &Bench) -> PathBuf {
    bench
        .worktree
        .parent()
        .expect("the worktree sits in the bench's world")
        .join("command-runs")
}

fn ran(bench: &Bench) -> usize {
    std::fs::read_to_string(counter(bench))
        .map(|runs| runs.lines().count())
        .unwrap_or(0)
}

/// A run parked on its comparison's regression: the suite ran twice, to
/// measure and to compare.
async fn parked_on_a_regression() -> Bench {
    let bench = Bench::new();
    write(&bench.worktree.join("marker.txt"), "ok");
    let config = format!(
        "{MOCK_CONFIG}baseline:\n  suite: \"echo . >> {}; cat marker.txt\"\n",
        counter(&bench).display()
    );
    let RunReport { terminal, .. } = bench
        .run_with_config(REGRESSES, "sessions: []\n", &config)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(ran(&bench), 2);
    bench
}

/// Every failure the log holds for `node`, oldest first.
fn failures(bench: &Bench, node: &str) -> Vec<Failure> {
    bench
        .events()
        .iter()
        .filter(|event| event.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(p))) => Some(p.failure.clone()),
            _ => None,
        })
        .collect()
}

async fn retried(bench: &Bench) -> RunTerminal {
    answer_parked(bench, "retry").await.unwrap();
    bench.wake_on_fixture("sessions: []\n").await.terminal
}

#[tokio::test]
async fn a_check_retried_on_the_tree_it_failed_on_fails_without_running_again() {
    let bench = parked_on_a_regression().await;

    let terminal = retried(&bench).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(ran(&bench), 2, "the suite did not run for the retry");
    let refused = failures(&bench, "compare").pop().expect("the retry failed");
    let Failure::Unchanged { unchanged } = &refused else {
        panic!("the retry is refused as unchanged: {refused:?}");
    };
    assert_eq!(unchanged.since, 1);
    assert!(
        unchanged.failure.to_string().starts_with("regression:"),
        "{refused}"
    );
}

#[tokio::test]
async fn a_check_retried_after_its_tree_changed_runs_and_can_pass() {
    let bench = parked_on_a_regression().await;
    write(&bench.worktree.join("marker.txt"), "fixed");

    assert_eq!(retried(&bench).await, RunTerminal::Finished);
    assert_eq!(ran(&bench), 3, "the retry ran the suite on the new tree");
}

/// A refusal is still a failure a person may change the tree for: the
/// menu offers the next attempt, and what it shows is what the attempt
/// that ran failed with.
#[tokio::test]
async fn a_refused_retry_still_offers_retry_and_names_the_attempt_that_ran() {
    let bench = parked_on_a_regression().await;
    retried(&bench).await;

    let (node, escalation) =
        current_escalation(&bench.manifest(), &yunta_engine::derive(&bench.events()))
            .expect("the refusal is a pause with a menu");
    assert_eq!(node.as_str(), "compare");
    let ids: Vec<&str> = escalation.options().iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["retry", "abort"]);
    assert_eq!(
        escalation.options()[0].label,
        "Run `compare` again (attempt 3)"
    );
    let said = escalation.evidence().lines().join("\n");
    assert!(
        said.contains("since attempt 1 failed") && said.contains("regression:"),
        "{said}"
    );
}

#[tokio::test]
async fn a_second_refusal_names_the_attempt_that_last_ran() {
    let bench = parked_on_a_regression().await;
    retried(&bench).await;
    retried(&bench).await;

    assert_eq!(ran(&bench), 2);
    let refusals: Vec<u32> = failures(&bench, "compare")
        .iter()
        .filter_map(|failure| match failure {
            Failure::Unchanged { unchanged } => Some(unchanged.since),
            _ => None,
        })
        .collect();
    assert_eq!(
        refusals,
        vec![1, 1],
        "both name attempt 1, the one that ran"
    );
}

#[tokio::test]
async fn a_coverage_gate_retried_on_an_unchanged_tree_fails_without_running_again() {
    let bench = Bench::new();
    let config = format!(
        "{MOCK_CONFIG}coverage:\n  cmd: \"echo . >> {}; echo 10%\"\n  threshold: 50.0\n",
        counter(&bench).display()
    );
    let workflow = "name: covered\nnodes:\n  - { id: gate, kind: check, builtin: coverage_gate }\n";
    let RunReport { terminal, .. } = bench
        .run_with_config(workflow, "sessions: []\n", &config)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    retried(&bench).await;

    assert_eq!(ran(&bench), 1, "only the first attempt measured coverage");
    assert!(matches!(
        failures(&bench, "gate").pop(),
        Some(Failure::Unchanged { .. })
    ));
}
