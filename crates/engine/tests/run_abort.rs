use yunta_core::events::Failure;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

/// Under `on_failure: abort` the run closes failed, and the report names
/// the node and carries its failure as data, not as a sentence.
#[tokio::test]
async fn a_run_that_aborts_on_a_failure_reports_the_failure_as_data() {
    let bench = Bench::new();
    let workflow = r#"
name: aborts
nodes:
  - id: build
    kind: bash
    run: "exit 3"
"#;
    let RunReport { terminal, .. } = bench
        .run_with_config(
            workflow,
            "dialect: claude-code\nsessions: []\n",
            "defaults:\n  on_failure: abort\n",
        )
        .await;
    assert!(
        matches!(terminal, RunTerminal::Failed { ref node, failure: Failure::Exited { .. } } if node.as_str() == "build"),
        "{terminal:?}"
    );
}
