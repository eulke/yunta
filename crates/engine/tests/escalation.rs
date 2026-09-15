//! `current_escalation` reconstructs the escalation
//! object a paused run is currently waiting on — purely from the
//! manifest and its own log, no live process required. This is what
//! lets `resolve_gate` (a separate `yunta mcp` invocation, not the
//! process that paused the run) know what it's answering.

use yunta_core::events::{GateEvent, NodeEvent, RunEvent};
use yunta_engine::{
    current_escalation, resolve_gate, ResolveGateError, RunReport, RunState, RunTerminal,
};
use yunta_testkit::Bench;
use yunta_testkit_core::FixedClock;

/// A bench whose run is parked: it drove `(workflow_yaml, fixture_yaml)`
/// with no human present and stopped on a pause, which is where every
/// test here starts.
async fn paused(workflow_yaml: &str, fixture_yaml: &str) -> Bench {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(workflow_yaml, fixture_yaml).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "expected the run to pause, got {terminal:?}"
    );
    bench
}

/// Records `option` as the answer to whatever escalation the bench's run
/// is parked on — the write a separate `yunta mcp` process makes, with
/// nothing of the run's own process behind it.
async fn resolve(bench: &Bench, option: &str) -> Result<(), ResolveGateError> {
    resolve_gate(
        &bench.manifest(),
        &bench.storage.async_handle(),
        &bench.run_id,
        &FixedClock,
        yunta_core::events::HumanChoice {
            option: option.into(),
            by: "mcp".into(),
            free_text: None,
        },
    )
    .await
}

/// How many of the run's events carry a payload `pred` accepts.
fn count(bench: &Bench, pred: impl Fn(&yunta_core::events::EventPayload) -> bool) -> usize {
    bench
        .events()
        .iter()
        .filter(|e| e.payload().is_some_and(&pred))
        .count()
}

/// The frozen manifest of a run parked with `NoInteraction` alongside its
/// log — exactly the two inputs a separate `resolve_gate` invocation
/// reads off disk (manifest.yaml + storage), with nothing else.
async fn paused_manifest_and_events(
    workflow_yaml: &str,
    fixture_yaml: &str,
) -> (yunta_core::Manifest, Vec<yunta_core::events::StoredEvent>) {
    let bench = paused(workflow_yaml, fixture_yaml).await;
    (bench.manifest(), bench.events())
}

const HOPELESS_UNTIL_RETRIED_WORKFLOW: &str = r#"
name: hopeless-until-retried
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 1 }
  - id: fix-lint
    kind: prompt
    runner: executor
    prompt: "Try to fix it."
"#;

const HOPELESS_UNTIL_RETRIED_FIXTURE: &str = r#"
sessions:
  - outcome: { type: completed, summary: "did nothing useful" }
"#;

#[tokio::test]
async fn reconstructs_an_exhausted_reroute_escalation_with_retry_and_abort() {
    let (manifest, events) = paused_manifest_and_events(
        HOPELESS_UNTIL_RETRIED_WORKFLOW,
        HOPELESS_UNTIL_RETRIED_FIXTURE,
    )
    .await;

    // Nothing was ever logged for this pause (no live surface) — the
    // whole point is that `current_escalation` rebuilds it without one.
    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::Gates(GateEvent::Waiting(
            _
        )))
    )));

    let (node_id, escalation) = current_escalation(&manifest, &yunta_engine::derive(&events))
        .expect("an exhausted re-route must reconstruct an escalation");
    assert_eq!(node_id.as_str(), "lint");
    assert!(escalation.summary().contains("lint"));
    assert!(escalation.summary().contains("fix-lint"));
    let ids: Vec<&str> = escalation.options().iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["retry", "abort"]);
    assert!(escalation.options().iter().all(|o| !o.tradeoff.is_empty()));
}

const INTERNAL_GATE_WORKFLOW: &str = r#"
name: internal-gate
nodes:
  - id: plan
    kind: bash
    run: "true"
  - id: approve
    kind: gate
    depends_on: [plan]
    assignee: lead
    message: "Approve the plan?"
    options: [aprobar, ajustar]
    on: { ajustar: plan }
"#;

#[tokio::test]
async fn reconstructs_an_internal_gate_escalation_with_its_declared_options() {
    let (manifest, events) =
        paused_manifest_and_events(INTERNAL_GATE_WORKFLOW, "sessions: []\n").await;

    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::Gates(GateEvent::Waiting(
            _
        )))
    )));

    let (node_id, escalation) = current_escalation(&manifest, &yunta_engine::derive(&events))
        .expect("an unresolved internal gate must reconstruct an escalation");
    assert_eq!(node_id.as_str(), "approve");
    assert_eq!(escalation.summary(), "Approve the plan?");
    let ids: Vec<&str> = escalation.options().iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["aprobar", "ajustar", "abort"]);
    assert!(escalation.options().iter().all(|o| !o.tradeoff.is_empty()));
}

#[tokio::test]
async fn a_plain_failure_with_no_on_failure_has_no_escalation_to_reconstruct() {
    // A pause with no menu of options (no on_failure, no gate) — nothing
    // for `resolve_gate` to answer; the caller keeps its plain pause
    // reason instead of inventing a decision that doesn't exist.
    let workflow = r#"
name: plain-failure
nodes:
  - id: broken
    kind: bash
    run: "false"
"#;
    let (manifest, events) = paused_manifest_and_events(workflow, "sessions: []\n").await;
    assert!(current_escalation(&manifest, &yunta_engine::derive(&events)).is_none());
}

// --- resolve_gate writes ONLY the decision; the engine
// consumes it on wake through its one existing consequence path.

const RETRY_FIX_FIXTURE: &str = r#"
sessions:
  - effects:
      - { path: fixed.txt, content: "fixed" }
    outcome: { type: completed, summary: "actually fixed it this time" }
"#;

#[tokio::test]
async fn a_pre_seeded_retry_is_consumed_by_a_plain_resume_and_finishes() {
    let bench = paused(
        HOPELESS_UNTIL_RETRIED_WORKFLOW,
        HOPELESS_UNTIL_RETRIED_FIXTURE,
    )
    .await;

    resolve(&bench, "retry").await.unwrap();
    // resolve_gate writes ONLY the decision pair — the reroute
    // consequence is the engine's to apply, not this function's.
    assert_eq!(
        count(&bench, |p| matches!(
            p,
            yunta_core::events::EventPayload::Node(NodeEvent::Rerouted(_))
        )),
        1,
        "only the run's own automatic reroute is on the log before the resume"
    );

    let RunReport { terminal, state } = bench.wake_on_fixture(RETRY_FIX_FIXTURE).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("lint"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));
    // The consuming engine never re-emits the recorded pair.
    assert_eq!(
        count(&bench, |p| matches!(
            p,
            yunta_core::events::EventPayload::Gates(GateEvent::Waiting(_))
        )),
        1
    );
    assert_eq!(
        count(&bench, |p| matches!(
            p,
            yunta_core::events::EventPayload::Gates(GateEvent::Resolved(_))
        )),
        1
    );
}

const PROMOTABLE_WORKFLOW: &str = r#"
name: promotable
modes:
  quick: { include: [lint, fix-lint] }
  full:  { include: [ship] }
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "true"
  - id: ship
    kind: bash
    run: "echo shipped > shipped.txt"
"#;

#[tokio::test]
async fn a_pre_seeded_promote_closes_the_run_as_promoted_on_resume() {
    // The paused run sits in mode `quick` with `full` later in the
    // declaration order — `promote` is on its menu. resolve_gate
    // records the choice; the resuming engine (a live process, exactly
    // what promotion's distill+close needs) applies it.
    let bench = Bench::new().in_mode("quick");
    let report = bench.run(PROMOTABLE_WORKFLOW, "sessions: []\n").await;
    assert!(matches!(report.terminal, RunTerminal::Paused { .. }));

    resolve(&bench, "promote").await.unwrap();

    let report = bench.wake_on_fixture("sessions: []\n").await;
    assert_eq!(
        report.terminal,
        RunTerminal::Promoted {
            suggested_mode: "full".into()
        }
    );
    let events = bench.events();
    assert!(events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::Run(
            RunEvent::PromotionSignaled(_)
        ))
    )));
    assert!(events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::Run(RunEvent::Finished(p))) if p.terminal_state == yunta_core::events::TerminalState::Promoted
    )));
}

const INTERNAL_GATE_DAG_WORKFLOW: &str = r#"
name: internal-gate
nodes:
  - id: plan
    kind: bash
    run: "sh -c 'echo run >> plan-runs.txt'"
  - id: approve
    kind: gate
    depends_on: [plan]
    assignee: lead
    message: "Approve the plan?"
    options: [aprobar, ajustar]
    on: { ajustar: plan }
  - id: ship
    kind: bash
    depends_on: [approve]
    run: "echo done > shipped.txt"
"#;

fn plan_runs(worktree: &std::path::Path) -> usize {
    std::fs::read_to_string(worktree.join("plan-runs.txt"))
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

#[tokio::test]
async fn a_pre_seeded_internal_gate_unmapped_option_finishes_the_gate_on_resume() {
    let bench = paused(INTERNAL_GATE_DAG_WORKFLOW, "sessions: []\n").await;

    resolve(&bench, "aprobar").await.unwrap();
    let RunReport { terminal, state } = bench.wake_on_fixture("sessions: []\n").await;

    assert_eq!(terminal, RunTerminal::Finished);
    match state.nodes.state("approve") {
        Some(yunta_engine::NodeState::Finished { outcome, .. }) => assert_eq!(outcome, "aprobar"),
        other => panic!("expected the gate finished with the chosen option, got {other:?}"),
    }
    assert!(bench.worktree.join("shipped.txt").exists());
    // One recorded pair — the consuming engine never re-emits it.
    assert_eq!(
        count(&bench, |p| matches!(
            p,
            yunta_core::events::EventPayload::Gates(GateEvent::Waiting(_))
        )),
        1
    );
    assert_eq!(
        count(&bench, |p| matches!(
            p,
            yunta_core::events::EventPayload::Gates(GateEvent::Resolved(_))
        )),
        1
    );
}

#[tokio::test]
async fn a_pre_seeded_internal_gate_mapped_option_reroutes_and_asks_again() {
    let bench = paused(INTERNAL_GATE_DAG_WORKFLOW, "sessions: []\n").await;
    assert_eq!(plan_runs(&bench.worktree), 1);

    resolve(&bench, "ajustar").await.unwrap();
    let RunReport { terminal, .. } = bench.wake_on_fixture("sessions: []\n").await;

    // A reroute through a pre-seeded choice: plan re-ran, the gate came
    // back to ask again, and with no live surface the run parks there.
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert_eq!(plan_runs(&bench.worktree), 2);
}

#[tokio::test]
async fn a_pre_seeded_abort_is_consumed_exactly_once() {
    let bench = paused(
        HOPELESS_UNTIL_RETRIED_WORKFLOW,
        HOPELESS_UNTIL_RETRIED_FIXTURE,
    )
    .await;

    resolve(&bench, "abort").await.unwrap();
    let RunReport { terminal, .. } = bench.wake_on_fixture("sessions: []\n").await;
    let RunTerminal::Paused { reason } = terminal else {
        panic!("expected the consumed abort to pause, got {terminal:?}");
    };
    assert_eq!(reason, "node `lint`'s gate was resolved to abort");

    // A later manual resume must NOT re-apply the stale abort — the
    // decision was consumed; with no live surface it parks on the
    // escalation again, exactly as an interactive abort does today.
    let RunReport { terminal, .. } = bench.wake_on_fixture("sessions: []\n").await;
    let RunTerminal::Paused { reason } = terminal else {
        panic!("expected the second resume to pause, got {terminal:?}");
    };
    assert!(
        !reason.contains("abort") && reason.contains("exhausted"),
        "a stale abort must not be re-applied: {reason}"
    );
    assert_eq!(
        count(&bench, |p| matches!(
            p,
            yunta_core::events::EventPayload::Gates(GateEvent::Resolved(_))
        )),
        1
    );
}

#[tokio::test]
async fn live_and_pre_seeded_retry_reach_the_same_final_state() {
    // As a property: the surface is presentation,
    // the decision is the data — answering live or by a later separate
    // process must land the run in the identical derived state.
    struct RetryOnce;
    #[async_trait::async_trait]
    impl yunta_engine::HumanInteraction for RetryOnce {
        async fn resolve(
            &self,
            _escalation: &yunta_core::events::GateWaitingPayload,
        ) -> Option<yunta_core::events::HumanChoice> {
            Some(yunta_core::events::HumanChoice {
                option: "retry".into(),
                by: "mcp".into(),
                free_text: None,
            })
        }
    }

    const TWO_SESSION_FIXTURE: &str = r#"
sessions:
  - outcome: { type: completed, summary: "did nothing useful" }
  - effects:
      - { path: fixed.txt, content: "fixed" }
    outcome: { type: completed, summary: "actually fixed it this time" }
"#;

    let live = Bench::new();
    let RunReport {
        terminal: live_terminal,
        state: live_state,
    } = live
        .run_with_interaction(
            HOPELESS_UNTIL_RETRIED_WORKFLOW,
            TWO_SESSION_FIXTURE,
            &RetryOnce,
        )
        .await;

    let seeded = paused(
        HOPELESS_UNTIL_RETRIED_WORKFLOW,
        HOPELESS_UNTIL_RETRIED_FIXTURE,
    )
    .await;
    resolve(&seeded, "retry").await.unwrap();
    let RunReport {
        terminal: seeded_terminal,
        state: seeded_state,
    } = seeded.wake_on_fixture(RETRY_FIX_FIXTURE).await;

    assert_eq!(live_terminal, seeded_terminal);
    // The state each node reached, not where its events landed: the
    // seeded log carries two events the live one does not — the pair a
    // `resolve_gate` wrote while the run was parked — so every later
    // `seq` differs by exactly that, and saying so proves nothing about
    // the two runs agreeing.
    let project = |state: &RunState| -> std::collections::BTreeMap<String, String> {
        state
            .nodes
            .iter()
            .map(|(id, record)| (id.to_string(), format!("{:?}", record.state)))
            .collect()
    };
    assert_eq!(project(&live_state), project(&seeded_state));
}

#[tokio::test]
async fn resolve_gate_refuses_a_run_that_is_not_parked() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run(
            r#"
name: fine
nodes:
  - id: ok
    kind: bash
    run: "true"
"#,
            "sessions: []\n",
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    let err = resolve(&bench, "retry").await.unwrap_err();
    assert!(matches!(err, ResolveGateError::NotPaused));
}

#[tokio::test]
async fn resolve_gate_rejects_an_option_not_on_the_menu() {
    let bench = paused(
        HOPELESS_UNTIL_RETRIED_WORKFLOW,
        HOPELESS_UNTIL_RETRIED_FIXTURE,
    )
    .await;
    let err = resolve(&bench, "nonexistent-option").await.unwrap_err();
    match err {
        ResolveGateError::UnknownOption { chosen, declared } => {
            assert_eq!(chosen, "nonexistent-option");
            assert_eq!(declared, "retry, abort");
        }
        other => panic!("expected UnknownOption, got {other:?}"),
    }
    // A refusal is a no-op on the log.
    assert_eq!(
        count(&bench, |p| matches!(
            p,
            yunta_core::events::EventPayload::Gates(GateEvent::Resolved(_))
        )),
        0
    );
}

#[tokio::test]
async fn resolve_gate_on_a_run_paused_without_a_menu_errors() {
    let bench = paused(
        r#"
name: plain-failure
nodes:
  - id: broken
    kind: bash
    run: "false"
"#,
        "sessions: []\n",
    )
    .await;
    let err = resolve(&bench, "anything").await.unwrap_err();
    assert!(matches!(err, ResolveGateError::NothingToResolve));
}
