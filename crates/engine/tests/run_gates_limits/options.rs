//! Gates that declare their options: choosing abort pauses the run, a gate
//! with no live surface pauses as it did before, and an internal gate's
//! approval resolves it and the dag continues.

use super::*;

#[tokio::test]
async fn a_gate_that_declares_abort_pauses_the_run_when_abort_is_chosen() {
    // Declaring `abort` places it on the menu; it never makes it an
    // option that lets the run go on past the gate.
    let bench = Bench::new();
    let interaction = ScriptedInteraction::new(yunta_core::events::HumanChoice {
        option: "abort".into(),
        by: "lead".into(),
        free_text: Some("not this plan".to_string()),
    });

    let RunReport { terminal, state } = bench
        .run_with_interaction(
            r#"
name: declared-abort
nodes:
  - id: plan
    kind: bash
    run: "true"
  - id: approve
    kind: gate
    depends_on: [plan]
    assignee: lead
    options: [proceed, abort, revise]
    on: { revise: plan }
  - id: ship
    kind: bash
    depends_on: [approve]
    run: "touch shipped"
"#,
            "dialect: claude-code\nsessions: []\n",
            &interaction,
        )
        .await;

    match terminal {
        RunTerminal::Paused {
            reason: PauseReason::GateAborted { node, free_text },
        } => {
            assert_eq!(node.as_str(), "approve");
            assert_eq!(free_text.as_deref(), Some("not this plan"));
        }
        other => panic!("expected Paused, got {other:?}"),
    }
    assert!(
        state.nodes.state("ship").is_none(),
        "the node after the gate never starts"
    );
    assert!(!bench.worktree.join("shipped").exists());
    let events = bench.events();
    let waiting = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Gates(GateEvent::Waiting(p))) => Some(p),
            _ => None,
        })
        .expect("the question and its answer are on the log");
    let offered: Vec<(&str, Option<String>)> = waiting
        .options()
        .iter()
        .map(|o| (o.id.as_str(), option_words(waiting, o).1))
        .collect();
    assert_eq!(
        offered,
        vec![
            (
                "proceed",
                Some("resolves this gate; `ship` runs next".to_string()),
            ),
            (
                "abort",
                Some("Pauses here; nothing further executes".to_string()),
            ),
            (
                "revise",
                Some("re-routes to `plan` and asks again once it completes".to_string()),
            ),
        ],
        "abort sits where the author put it and says what it does"
    );
}

#[tokio::test]
async fn a_gate_with_no_live_interaction_degrades_to_pausing_exactly_as_before() {
    // Regression: `NoInteraction` (what every other test in this suite
    // already uses) must reproduce the same pause behavior byte for
    // byte — a gate existing must never change what an unattended run
    // does.
    let bench = Bench::new();
    let RunReport { terminal, state: _ } = bench
        .run(
            HOPELESS_UNTIL_RETRIED_WORKFLOW,
            HOPELESS_UNTIL_RETRIED_FIXTURE,
        )
        .await;

    match terminal {
        RunTerminal::Paused {
            reason: reason @ PauseReason::Escalation(_),
        } => assert_eq!(
            yunta_render::pause::sentence(&reason),
            "node `lint` failed and its 1 re-route to `fix-lint` is exhausted — exit 1"
        ),
        other => panic!("expected Paused, got {other:?}"),
    }
}

#[tokio::test]
async fn an_internal_gate_approved_resolves_and_the_dag_continues() {
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["proceed"]);
    let RunReport { terminal, state } = bench
        .run_with_interaction(
            INTERNAL_GATE_WORKFLOW,
            "dialect: claude-code\nsessions: []\n",
            &interaction,
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    match state.nodes.state("approve") {
        Some(yunta_engine::NodeState::Finished { outcome, .. }) => {
            assert_eq!(
                outcome,
                &NodeOutcome::Fact(EngineFact::OptionChosen {
                    option: yunta_core::OptionId::from_static("proceed"),
                })
            )
        }
        other => panic!("expected the gate finished with the chosen option, got {other:?}"),
    }
    assert_eq!(plan_run_count(&bench.worktree), 1);

    // The recorded escalation carries the declared options plus the
    // engine-appended abort, each with a tradeoff.
    let events = bench.events();
    let waiting = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Gates(GateEvent::Waiting(p))) => Some(p),
            _ => None,
        })
        .expect("the resolved interaction must be on the log");
    let ids: Vec<&str> = waiting.options().iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["proceed", "revise", "abort"]);
    let words = |o| option_words(waiting, o).1;
    assert!(waiting.options().iter().all(|o| words(o).is_some()));
    assert_eq!(waiting.summary(), "Approve the plan?");
}
