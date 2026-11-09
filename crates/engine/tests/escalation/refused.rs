//! What `resolve_gate` refuses: a run not parked, an option off its menu,
//! and a pause with no menu to answer. A refusal writes nothing.

use super::*;
use yunta_core::refusal::GateRefusal;
use yunta_engine::ResolveGateError;

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
            NO_SESSIONS,
        )
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    let err = answer_parked(&bench, "retry").await.unwrap_err();
    assert!(matches!(
        err,
        ResolveGateError::Refused(GateRefusal::NotPaused)
    ));
}

#[tokio::test]
async fn resolve_gate_rejects_an_option_not_on_the_menu() {
    let bench = parked(
        HOPELESS_UNTIL_RETRIED_WORKFLOW,
        HOPELESS_UNTIL_RETRIED_FIXTURE,
    )
    .await;
    let err = answer_parked(&bench, "nonexistent-option")
        .await
        .unwrap_err();
    match err {
        ResolveGateError::Refused(GateRefusal::UnknownOption { chosen, declared }) => {
            assert_eq!(chosen, "nonexistent-option");
            assert_eq!(declared, "retry, abort");
        }
        other => panic!("expected UnknownOption, got {other:?}"),
    }
    // A refusal is a no-op on the log.
    assert_eq!(
        events_matching(&bench, |p| matches!(
            p,
            yunta_core::events::EventPayload::Gates(GateEvent::Resolved(_))
        )),
        0
    );
}

#[tokio::test]
async fn resolve_gate_on_a_run_paused_without_a_menu_errors() {
    // A node interrupted mid-attempt under `fail_if_uncertain`: the run
    // parks for a person to look at the tree, and there is no option to
    // choose — only the tree to inspect.
    let bench = Bench::new();
    bench
        .create(
            r#"
name: uncertain
nodes:
  - id: only
    kind: bash
    on_interrupt: fail_if_uncertain
    run: "true"
"#,
            NO_SESSIONS,
            yunta_testkit::MOCK_CONFIG,
        )
        .await;
    bench
        .storage
        .append(
            &yunta_core::events::EventDraft::new(
                bench.run_id.clone(),
                Some("only".into()),
                yunta_core::events::EventPayload::Node(NodeEvent::Started(
                    yunta_core::events::NodeStartedPayload::attempt(1),
                )),
            )
            .expect("a draft names its subject"),
            &FixedClock,
        )
        .unwrap();
    let RunReport { terminal, .. } = bench.wake().await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let err = answer_parked(&bench, "anything").await.unwrap_err();
    assert!(matches!(
        err,
        ResolveGateError::Refused(GateRefusal::NothingToResolve)
    ));
}
