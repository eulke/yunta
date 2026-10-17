//! A node's failure in the run's own words: a session that never hands over
//! its declared document fails its node, and a loop node whose session dies
//! names the adapter and the exit it died with.

use super::*;

#[tokio::test]
async fn a_session_that_never_hands_over_its_declared_document_fails_the_node() {
    let bench = Bench::new();

    let workflow = r#"
name: broken-plan
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the plan."
    artifacts:
      produces: [tasks]
"#;

    // The session claims success but submits nothing — the engine
    // verifies, and the document nobody handed over fails the node. It
    // names the node and what that node owes: no file was ever going to
    // be there, so the failure points at nothing on disk.
    let fixture = r#"
dialect: claude-code
sessions:
  - outcome: { type: completed, summary: "trust me, it is written" }
"#;

    let RunReport { terminal, state } = bench.run(workflow, fixture).await;

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    match state.nodes.state("plan") {
        Some(NodeState::Failed { failure, .. }) => {
            assert!(
                matches!(failure, Failure::Artifacts { .. }),
                "a document nobody handed over is an artifact failure: {failure:?}"
            );
            assert_eq!(
                yunta_render::failure::sentence(failure),
                "node `plan`: 1 error\n  handed over no tasks document — \
                 produce it before the node ends, or stop declaring it here"
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

/// A plan that writes the tasks document, then a loop that implements it.
const DYING_LOOP_WORKFLOW: &str = r#"
name: dying-loop
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#;

/// The planner submits one task; the loop's executor session then crashes.
const DYING_LOOP_FIXTURE: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            tasks:
              - id: T001
                title: "Create hello"
                scope: ["hello.txt"]
                criteria:
                  - cmd: "test -f hello.txt"
    outcome: { type: completed, summary: "planned" }
  - outcome: { type: crash }
"#;

/// A `loop` node whose executor session dies reports the death, not the
/// tail that says no task is ready: what a reader has to act on is the
/// CLI that would not start, and the criteria were never run at all.
#[tokio::test]
async fn a_loop_node_whose_session_died_fails_naming_the_adapter_and_the_exit() {
    let bench = Bench::new();

    let RunReport { terminal, state: _ } = bench.run(DYING_LOOP_WORKFLOW, DYING_LOOP_FIXTURE).await;
    assert!(
        !matches!(terminal, RunTerminal::Finished),
        "a loop whose session died does not finish: {terminal:?}"
    );

    let failure = bench
        .events()
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Node(NodeEvent::Failed(p))) => Some(p.clone()),
            _ => None,
        })
        .expect("the loop node failed");
    let yunta_core::events::Failure::TasksStopped { stopped } = &failure.failure else {
        panic!("the death is the stop, not a tail: {:?}", failure.failure);
    };
    let [yunta_core::events::TaskStop {
        cause: yunta_core::events::TaskCause::SessionDied(died),
        ..
    }] = stopped.as_slice()
    else {
        panic!("one task stopped on its death: {stopped:?}");
    };
    assert_eq!(died.adapter, "mock");
    let (_, escalation) = current_escalation(
        &bench.manifest(),
        &bench.run_dir(),
        &yunta_engine::derive(&bench.events()),
    )
    .await
    .unwrap()
    .expect("a loop whose session died is a decision");
    let ids: Vec<&str> = escalation.options().iter().map(|o| o.id.as_str()).collect();
    assert!(
        ids.contains(&"retry"),
        "a CLI that would not start is worth another run: {ids:?}"
    );
}
