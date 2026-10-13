//! A check that judges the run's tree answers the same on the same tree.
//! A regression offers abort alone on the tree it failed on, as a red
//! coverage gate does. A retry comes back once a wake finds the tree
//! changed, and a retry chosen while the tree moved runs only if the menu
//! still offers it when the run wakes.

use std::path::PathBuf;

use yunta_core::events::{EventPayload, Failure, NodeEvent, RedCheck};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{write, Bench, MOCK_CONFIG};

mod common;
use common::answer_parked;
use common::menu::rebuilt_menu;

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

/// The fixture of a run that scripts no session, so a wake reaches only its gates.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

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

/// Removes the marker file, which a person moves the tree back by hand.
fn remove_marker(bench: &Bench) {
    std::fs::remove_file(bench.worktree.join("marker.txt")).expect("the tree moves back");
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
    let RunReport { terminal, .. } = bench.run_with_config(REGRESSES, NO_SESSIONS, &config).await;
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

/// The ids of the options the menu a paused run offers now, rebuilt from
/// its log the way a wake rebuilds it.
async fn offered(bench: &Bench) -> Vec<String> {
    let (_, escalation) = rebuilt_menu(&bench.manifest(), &bench.run_dir(), &bench.events())
        .await
        .expect("the run is paused on a menu");
    escalation
        .options()
        .iter()
        .map(|option| option.id.as_str().to_string())
        .collect()
}

async fn offers_retry(bench: &Bench) -> bool {
    offered(bench).await.iter().any(|id| id == "retry")
}

/// Answers the menu with `retry`, then wakes the run headlessly.
async fn retried(bench: &Bench) -> RunTerminal {
    answer_parked(bench, "retry").await.unwrap();
    bench.wake_on_fixture(NO_SESSIONS).await.terminal
}

/// A regression offers abort alone on the tree it failed on, as a red
/// coverage gate does: a retry the menu does not offer is refused, and a
/// wake that follows runs no suite.
#[tokio::test]
async fn a_regression_offers_abort_alone_on_the_tree_it_failed_on() {
    let bench = parked_on_a_regression().await;
    let failure = failures(&bench, "compare")
        .pop()
        .expect("the comparison failed");
    let Failure::Red {
        red: RedCheck::Regression {
            exit_code, tail, ..
        },
    } = &failure
    else {
        panic!("the comparison fails on a regression: {failure:?}");
    };
    assert_eq!(*exit_code, 1);
    assert!(
        tail.iter().any(|line| line.contains("No such file")),
        "the regression keeps what the suite printed: {tail:?}"
    );
    assert_eq!(offered(&bench).await, ["abort"]);
    assert!(
        matches!(
            answer_parked(&bench, "retry").await,
            Err(yunta_engine::ResolveGateError::UnknownOption { .. })
        ),
        "a retry the menu does not offer is refused"
    );

    let RunReport { terminal, .. } = bench.wake_on_fixture(NO_SESSIONS).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(
        ran(&bench),
        2,
        "the suite does not run again on the tree it failed on"
    );
    assert_eq!(offered(&bench).await, ["abort"]);
}

/// A person who writes the tree back and a wake that finds the change
/// offer the retry again, and choosing it runs the suite once more.
#[tokio::test]
async fn a_retry_is_offered_once_a_wake_finds_the_tree_changed_and_runs_the_suite() {
    let bench = parked_on_a_regression().await;
    // The baseline passed on the tree with `ok` in the marker, and a tree
    // it passed on is answered from that measurement: the marker takes
    // content no measurement saw, so the retry runs the suite.
    write(&bench.worktree.join("marker.txt"), "fixed");
    let RunReport { terminal, .. } = bench.wake_on_fixture(NO_SESSIONS).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert!(
        offers_retry(&bench).await,
        "the menu offers the retry once the tree changed"
    );
    assert_eq!(
        ran(&bench),
        2,
        "a wake runs nothing the person did not choose"
    );

    assert_eq!(retried(&bench).await, RunTerminal::Finished);
    assert_eq!(
        ran(&bench),
        3,
        "the retry ran the suite on the changed tree"
    );
}

/// A retry chosen while the tree moved does not run once the tree moved
/// back: the wake that finds it pauses again, and the menu offers abort
/// alone.
#[tokio::test]
async fn a_retry_chosen_on_a_moved_tree_is_not_run_once_the_tree_moved_back() {
    let bench = parked_on_a_regression().await;
    write(&bench.worktree.join("marker.txt"), "fixed");
    bench.wake_on_fixture(NO_SESSIONS).await;
    assert!(
        offers_retry(&bench).await,
        "the menu offers the retry once the tree changed"
    );
    answer_parked(&bench, "retry")
        .await
        .expect("the retry is chosen while the menu offers it");
    remove_marker(&bench);

    let RunReport { terminal, .. } = bench.wake_on_fixture(NO_SESSIONS).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(
        ran(&bench),
        2,
        "the chosen retry does not run on the tree it failed on"
    );
    assert_eq!(offered(&bench).await, ["abort"]);
}

/// A red coverage gate offers no retry on the tree it failed on, and a
/// resume measures nothing again.
#[tokio::test]
async fn a_coverage_gate_on_an_unchanged_tree_offers_no_retry_and_runs_no_more() {
    let bench = Bench::new();
    let config = format!(
        "{MOCK_CONFIG}coverage:\n  cmd: \"echo . >> {}; echo 10%\"\n  threshold: 50.0\n",
        counter(&bench).display()
    );
    let workflow = "name: covered\nnodes:\n  - { id: gate, kind: check, builtin: coverage_gate }\n";
    let RunReport { terminal, .. } = bench.run_with_config(workflow, NO_SESSIONS, &config).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    assert!(
        matches!(
            answer_parked(&bench, "retry").await,
            Err(yunta_engine::ResolveGateError::UnknownOption { .. })
        ),
        "no retry is offered on the tree the gate failed on"
    );
    let RunReport { terminal, .. } = bench.wake_on_fixture(NO_SESSIONS).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    assert_eq!(ran(&bench), 1, "only the first attempt measured coverage");
    assert!(matches!(
        failures(&bench, "gate").pop(),
        Some(Failure::Red { .. })
    ));
}
