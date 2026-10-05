//! `modes:` exercised end-to-end: a node a mode excludes is never
//! scheduled; an in-mode node that depended on an excluded node waits
//! for the excluded node's own in-mode dependencies instead (the
//! "quick"/"standard"/"full" example does exactly this — `implement`
//! depends_on the excluded `approve-plan` in "quick", so it waits for
//! `plan`); and the run finishes once every *included* node is done,
//! never waiting on one that was never going to run.

use yunta_core::events::NodeEvent;
use yunta_engine::{NodeState, RunError, RunReport, RunTerminal};
use yunta_testkit::Bench;

const WORKFLOW: &str = r#"
name: mode-scenario
modes:
  quick:  { include: [start, ship] }
  full:   { include: all }
nodes:
  - id: start
    kind: bash
    run: "true"
  - id: extra
    kind: bash
    depends_on: [start]
    run: "true"
  - id: ship
    kind: bash
    depends_on: [extra]
    run: "true"
"#;

const FIXTURE: &str = "sessions: []\n";

#[tokio::test]
async fn quick_mode_skips_the_excluded_node_and_still_finishes() {
    let bench = Bench::with_run_id("run-quick").in_mode("quick");
    let RunReport { terminal, state } = bench.run(WORKFLOW, FIXTURE).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("start"),
        Some(NodeState::Finished { .. })
    ));
    assert!(matches!(
        state.nodes.state("ship"),
        Some(NodeState::Finished { .. })
    ));
    assert!(
        !state.nodes.has_state("extra"),
        "a node excluded from the run's mode must never be scheduled at all"
    );
}

#[tokio::test]
async fn full_mode_runs_every_node() {
    let bench = Bench::with_run_id("run-full").in_mode("full");
    let RunReport { terminal, state } = bench.run(WORKFLOW, FIXTURE).await;
    assert_eq!(terminal, RunTerminal::Finished);
    for id in ["start", "extra", "ship"] {
        assert!(
            matches!(state.nodes.state(id), Some(NodeState::Finished { .. })),
            "node `{id}` should have finished under `full`, got {:?}",
            state.nodes.state(id)
        );
    }
}

#[tokio::test]
async fn the_default_sentinel_ignores_modes_and_runs_everything() {
    // `"default"` never validates against `modes:` and never filters —
    // `yunta test` relies on exactly this to exercise a moded
    // workflow's full graph without picking one mode out from under it.
    let bench = Bench::with_run_id("run-default");
    let RunReport { terminal, state } = bench.run(WORKFLOW, FIXTURE).await;
    assert_eq!(terminal, RunTerminal::Finished);
    for id in ["start", "extra", "ship"] {
        assert!(matches!(
            state.nodes.state(id),
            Some(NodeState::Finished { .. })
        ));
    }
}

#[tokio::test]
async fn an_unknown_mode_name_is_refused_before_the_run_is_created() {
    let bench = Bench::with_run_id("run-bogus").in_mode("nonexistent");
    let err = bench.try_run(WORKFLOW, FIXTURE).await.unwrap_err();
    assert!(matches!(err, RunError::UnknownMode { .. }), "got: {err:?}");
    // Refused before anything was written — no run directory, no event.
    assert!(bench.storage.list_runs().unwrap().is_empty());
}

/// Declared so that the dependent of the excluded node comes *first* in
/// declaration order: an edge merely "treated as satisfied" would make
/// `ship` ready before `start` ran at all.
const ORDER_WORKFLOW: &str = r#"
name: mode-order
modes:
  quick:  { include: [ship, start] }
  full:   { include: all }
nodes:
  - id: ship
    kind: bash
    depends_on: [extra]
    run: "true"
  - id: extra
    kind: bash
    depends_on: [start]
    run: "true"
  - id: start
    kind: bash
    run: "true"
"#;

const GATE_WORKFLOW: &str = r#"
name: mode-gate
modes:
  quick:  { include: [start, ship] }
  full:   { include: all }
nodes:
  - id: start
    kind: bash
    run: "true"
  - id: extra
    kind: bash
    depends_on: [start]
    run: "true"
  - id: ship
    kind: gate
    depends_on: [extra]
    assignee: lead
    message: "Ship it?"
"#;

fn seq_of(
    events: &[yunta_core::events::StoredEvent],
    node: &str,
    pick: impl Fn(&yunta_core::events::EventPayload) -> bool,
) -> u64 {
    events
        .iter()
        .find(|e| {
            e.node_id.as_ref().is_some_and(|id| id.as_str() == node)
                && e.payload().is_some_and(&pick)
        })
        .unwrap_or_else(|| panic!("no matching event for node `{node}`"))
        .seq
        .get()
}

#[tokio::test]
async fn a_dependent_of_an_excluded_node_waits_for_that_nodes_own_dependencies() {
    let bench = Bench::with_run_id("run-order").in_mode("quick");
    let RunReport { terminal, .. } = bench.run(ORDER_WORKFLOW, FIXTURE).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.events();
    use yunta_core::events::EventPayload;
    let start_finished = seq_of(&events, "start", |p| {
        matches!(p, EventPayload::Node(NodeEvent::Finished(_)))
    });
    let ship_started = seq_of(&events, "ship", |p| {
        matches!(p, EventPayload::Node(NodeEvent::Started(_)))
    });
    assert!(
        start_finished < ship_started,
        "`ship` (depends_on the excluded `extra`, which depends_on `start`) started at seq \
         {ship_started}, before `start` finished at seq {start_finished}"
    );
}

#[tokio::test]
async fn a_gate_behind_an_excluded_node_waits_for_that_nodes_own_dependencies() {
    // With nobody to answer it, the internal gate pauses the run — but
    // only once everything it effectively depends on has run.
    let bench = Bench::with_run_id("run-gate").in_mode("quick");
    let RunReport { terminal, state } = bench.run(GATE_WORKFLOW, FIXTURE).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "expected the unanswered gate to pause the run, got {terminal:?}"
    );
    assert!(
        matches!(state.nodes.state("start"), Some(NodeState::Finished { .. })),
        "`start` must finish before the gate that transitively depends on it is asked; got {:?}",
        state.nodes.state("start")
    );
}

/// A failing root (`boom`) with a dependent (`after`), plus an
/// independent chain (`side` -> `tail`). `boom` declares no `on_failure`
/// of its own, so `defaults.on_failure` decides what the run does.
const ON_FAILURE_WORKFLOW: &str = r#"
name: on-failure
nodes:
  - id: boom
    kind: bash
    run: "false"
  - id: after
    kind: bash
    depends_on: [boom]
    run: "true"
  - id: side
    kind: bash
    run: "true"
  - id: tail
    kind: bash
    depends_on: [side]
    run: "true"
"#;

#[tokio::test]
async fn on_failure_abort_ends_the_run() {
    // `abort`: the first failed node with no re-route of its own closes
    // the run as failed at once — nothing new starts after it, not even a
    // node whose own dependencies are already satisfied (`tail`).
    let bench = Bench::with_run_id("run-abort");
    let RunReport { terminal, state } = bench
        .run_with_config(
            ON_FAILURE_WORKFLOW,
            FIXTURE,
            "defaults:\n  on_failure: abort\n",
        )
        .await;

    assert!(
        matches!(terminal, RunTerminal::Failed { .. }),
        "abort closes the run as failed, got {terminal:?}"
    );
    assert!(
        matches!(state.nodes.state("boom"), Some(NodeState::Failed { .. })),
        "the causing node is failed: {:?}",
        state.nodes.state("boom")
    );
    assert_eq!(
        state.nodes.state("tail"),
        None,
        "abort starts nothing after the failure, even a ready node"
    );
    assert_eq!(state.nodes.state("after"), None, "the dependent never ran");
}

#[tokio::test]
async fn on_failure_continue_skips_dependents() {
    // `continue`: the failed node's dependents are skipped (`after`), the
    // rest of the graph runs to completion (`side` -> `tail`), and the run
    // still closes as failed at the end.
    let bench = Bench::with_run_id("run-continue");
    let RunReport { terminal, state } = bench
        .run_with_config(
            ON_FAILURE_WORKFLOW,
            FIXTURE,
            "defaults:\n  on_failure: continue\n",
        )
        .await;

    assert!(
        matches!(terminal, RunTerminal::Failed { .. }),
        "continue still closes the run as failed, got {terminal:?}"
    );
    assert!(
        matches!(state.nodes.state("boom"), Some(NodeState::Failed { .. })),
        "the causing node is failed: {:?}",
        state.nodes.state("boom")
    );
    assert_eq!(
        state.nodes.state("after"),
        None,
        "a dependent of the failed node is skipped, never run"
    );
    assert!(
        matches!(state.nodes.state("side"), Some(NodeState::Finished { .. })),
        "an independent node runs: {:?}",
        state.nodes.state("side")
    );
    assert!(
        matches!(state.nodes.state("tail"), Some(NodeState::Finished { .. })),
        "a dependent of an independent node runs: {:?}",
        state.nodes.state("tail")
    );
}
