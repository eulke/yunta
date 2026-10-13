//! A document the session never hands over, and a typed artifact an adapter
//! without run tools cannot deliver: each fails its node, and the failure
//! names what is missing.

use super::*;

#[tokio::test]
async fn a_document_never_submitted_fails_the_node_with_no_second_session() {
    let bench = Bench::new();
    let fixture = r#"
dialect: claude-code
sessions:
  - outcome: { type: completed, summary: "said it was done" }
"#;
    let RunReport { terminal, state } = bench.run(PLAN_NODE, fixture).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }), "{state:?}");

    // One attempt, and no session opened to correct anything: a document
    // nobody handed over is a node that failed, not a node to re-instruct.
    assert_eq!(kinds(&bench, "node_started"), 1);
    assert_eq!(kinds(&bench, "agent_session_opened"), 1);

    let failure = bench
        .events()
        .iter()
        .rev()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(p))) => Some(p.failure.clone()),
            _ => None,
        })
        .expect("the node failed");
    assert!(matches!(failure, Failure::Artifacts { .. }), "{failure:?}");
    let (_, escalation) = current_escalation(
        &bench.manifest(),
        &bench.run_dir(),
        &yunta_engine::derive(&bench.events()),
    )
    .await
    .unwrap()
    .expect("the failure is a pause with a menu");
    let ids: Vec<&str> = escalation.options().iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["abort"], "there is nothing to attempt again");
}

#[tokio::test]
async fn a_typed_artifact_on_an_adapter_without_run_tools_fails_before_dispatch() {
    // The capability is absent, so the document has no way in. The node
    // fails before a session opens rather than after one produced
    // nothing.
    let bench = Bench::new().with_capabilities(Capabilities::default());
    let fixture = r#"
dialect: claude-code
sessions:
  - outcome: { type: completed, summary: "never reached" }
"#;
    let RunReport { terminal, state } = bench.run(PLAN_NODE, fixture).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }), "{state:?}");

    match state.nodes.state("plan") {
        Some(NodeState::Failed { failure, .. }) => {
            let text = yunta_render::failure::sentence(failure);
            assert!(
                text.contains("run_tools") && text.contains("tasks"),
                "the refusal names the capability and the artifact: {text}"
            );
        }
        other => panic!("expected plan failed, got {other:?}"),
    }
    assert_eq!(kinds(&bench, "agent_session_opened"), 0);
    assert_eq!(kinds(&bench, "capability_degraded"), 0);
}
