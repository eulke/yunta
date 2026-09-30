//! `scope: run` — a node that corrects the run's own work may change what
//! the run changed, as it stood when the node started, and nothing else.

use yunta_core::events::{EventPayload, Failure, NodeEvent, StoredEvent};
use yunta_core::ScopeGlob;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{write, Bench};

mod common;
use common::{answer_parked, parked};

/// What each start of `node` recorded it may change, oldest first.
fn run_scopes(events: &[StoredEvent], node: &str) -> Vec<Option<Vec<ScopeGlob>>> {
    events
        .iter()
        .filter(|event| event.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Started(p))) => Some(p.run_scope.clone()),
            _ => None,
        })
        .collect()
}

fn failures(events: &[StoredEvent], node: &str) -> Vec<Failure> {
    events
        .iter()
        .filter(|event| event.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(p))) => Some(p.failure.clone()),
            _ => None,
        })
        .collect()
}

fn globs(paths: &[&str]) -> Option<Vec<ScopeGlob>> {
    Some(paths.iter().map(|path| ScopeGlob::from(*path)).collect())
}

/// A node that writes, then one scoped to the run that writes `file`.
fn make_then_fix(file: &str) -> String {
    format!(
        "name: fixes\nnodes:\n  - {{ id: make, kind: bash, run: \"echo made > made.txt\" }}\n  - {{ id: fix, kind: bash, scope: run, depends_on: [make], run: \"echo fixed >> {file}\" }}\n"
    )
}

#[tokio::test]
async fn a_scope_run_node_may_write_what_the_run_changed() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run(&make_then_fix("made.txt"), "sessions: []\n")
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        run_scopes(&bench.events(), "fix"),
        vec![globs(&["made.txt"])]
    );
}

/// Writing a file the run never touched is outside the node's reach; a
/// person may still widen it, as for any declared scope.
#[tokio::test]
async fn writing_elsewhere_fails_with_a_grantable_violation() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run(&make_then_fix("elsewhere.txt"), "sessions: []\n")
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let failed = failures(&bench.events(), "fix");
    assert_eq!(
        failed,
        vec![Failure::scope_violated(vec!["elsewhere.txt".into()])]
    );
    assert!(failed[0].wants_scope(), "a person may grant it");
}

/// Scoped to a run that has changed nothing, a node may write nothing.
#[tokio::test]
async fn a_run_that_changed_nothing_leaves_nothing_writable() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run(
            "name: first\nnodes:\n  - { id: fix, kind: bash, scope: run, run: \"echo x > x.txt\" }\n",
            "sessions: []\n",
        )
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(run_scopes(&bench.events(), "fix"), vec![globs(&[])]);
    assert_eq!(
        failures(&bench.events(), "fix"),
        vec![Failure::scope_violated(vec!["x.txt".into()])]
    );
}

/// What a person changed in the run's tree while it was parked is part
/// of what the run changed: the next attempt may fix it.
#[tokio::test]
async fn a_persons_edit_is_part_of_what_the_run_changed() {
    let workflow = "name: fixes\nnodes:\n  - { id: fix, kind: bash, scope: run, run: \"test -f draft.txt && echo fixed >> draft.txt\" }\n";
    let bench = parked(workflow, "sessions: []\n").await;
    write(&bench.worktree.join("draft.txt"), "a person's draft\n");

    answer_parked(&bench, "retry").await.unwrap();
    let RunReport { terminal, .. } = bench.wake_on_fixture("sessions: []\n").await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        run_scopes(&bench.events(), "fix"),
        vec![globs(&[]), globs(&["draft.txt"])]
    );
}
