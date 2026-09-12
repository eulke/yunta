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

/// A node with both kinds of artifact: the ledger it hands over through
/// the run tools, and a file of its own the session writes. The opaque
/// one is what earns the session its reach into `artifacts/` — a node
/// that only submits needs none, so it never gets one, and there would
/// be nothing for this audit to catch.
const PLAN_AND_NOTES: &str = r#"
name: plan-and-notes
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write a task ledger and leave your notes."
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
        - notes.md
"#;

/// A session that submits its ledger and writes `effects` besides — one
/// `path: content` pair per line, already indented for the script.
fn fixture(effects: &str) -> String {
    format!(
        r#"
capabilities: {{ run_tools: true }}
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_task_ledger
        arguments:
          name: plan.yaml
          document:
            tasks:
              - id: t1
                title: "Work"
                scope: ["src/**"]
                criteria:
                  - cmd: "cargo test"
    effects:
{effects}
    outcome: {{ type: completed, summary: planned }}
"#
    )
}

/// One `effects` entry, at the indentation [`fixture`] splices it into.
fn effect(bench: &Bench, name: &str, content: &str) -> String {
    let path = bench.run_dir().join("artifacts").join(name);
    format!(
        "      - {{ path: \"{}\", content: \"{content}\" }}",
        path.display()
    )
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
    let effects = format!(
        "{}\n{}",
        effect(&bench, "notes.md", "what I did\\n"),
        effect(&bench, "findings-reviewer.yaml", "findings: []\\n"),
    );

    let (terminal, _state) = bench.run(PLAN_AND_NOTES, &fixture(&effects)).await;
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
        !failure.contains("notes.md") && !failure.contains("plan.yaml"),
        "the node's own artifacts are the node doing its job: {failure}"
    );
}

#[tokio::test]
async fn a_node_that_writes_only_what_it_declared_finishes() {
    let bench = Bench::new();
    let effects = effect(&bench, "notes.md", "what I did\\n");

    let (terminal, _state) = bench.run(PLAN_AND_NOTES, &fixture(&effects)).await;
    assert_eq!(terminal, RunTerminal::Finished);
}
