//! The run's `artifacts/` directory, audited at node close.
//!
//! Every node of a run writes into one shared directory, and a CLI
//! grants writes by directory rather than by file — so a node that
//! declares an artifact can reach every other node's. The worktree has
//! the same shape and the run answers it the same way: the session may
//! write, and the close audits what it wrote against what the node
//! declared.

use yunta_core::events::EventPayload;
use yunta_engine::RunTerminal;
use yunta_testkit::Bench;

const PLAN_ONLY: &str = r#"
name: plan-only
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write a task ledger."
    artifacts:
      produces: [{ name: plan.yaml, kind: task-ledger }]
"#;

const LEDGER: &str =
    "tasks:\\n  - id: t1\\n    title: Work\\n    scope: [\\\"src/**\\\"]\\n    criteria:\\n      - cmd: \\\"cargo test\\\"\\n";

fn artifact_path(bench: &Bench, name: &str) -> String {
    bench
        .run_dir()
        .join("artifacts")
        .join(name)
        .display()
        .to_string()
}

fn last_failure(events: &[yunta_core::events::StoredEvent]) -> String {
    events
        .iter()
        .rev()
        .find_map(|e| match e.payload() {
            Some(EventPayload::NodeFailed(p)) => Some(p.failure.to_string()),
            _ => None,
        })
        .expect("the node failed")
}

#[tokio::test]
async fn a_node_that_writes_an_artifact_it_never_declared_fails() {
    let bench = Bench::new();
    let declared = artifact_path(&bench, "plan.yaml");
    let someone_elses = artifact_path(&bench, "findings-reviewer.yaml");
    let fixture = format!(
        "sessions:\n  \
         - effects:\n      \
         - {{ path: \"{declared}\", content: \"{LEDGER}\" }}\n      \
         - {{ path: \"{someone_elses}\", content: \"findings: []\\n\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n"
    );

    let (terminal, _state) = bench.run(PLAN_ONLY, &fixture).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "a node does not get to write another node's artifact: {terminal:?}"
    );

    let failure = last_failure(&bench.storage.events_for_run(&bench.run_id).unwrap());
    assert!(
        failure.contains("findings-reviewer.yaml"),
        "the failure names the file that was not declared: {failure}"
    );
    assert!(
        !failure.contains("plan.yaml"),
        "the node's own artifact is the node doing its job: {failure}"
    );
}

#[tokio::test]
async fn a_node_that_writes_only_what_it_declared_finishes() {
    let bench = Bench::new();
    let declared = artifact_path(&bench, "plan.yaml");
    let fixture = format!(
        "sessions:\n  \
         - effects:\n      \
         - {{ path: \"{declared}\", content: \"{LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n"
    );

    let (terminal, _state) = bench.run(PLAN_ONLY, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
}
