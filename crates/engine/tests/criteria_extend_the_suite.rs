//! A command that runs the project's tests extends the suite the run
//! measures, so the build the suite leaves serves it: a plan that runs
//! them another way is refused when it is handed over, naming the
//! command, and the session writing the plan is told which suite to
//! extend.

use yunta_core::events::{ArtifactEvent, EventPayload, SubmissionOutcome};
use yunta_testkit::{git, Bench, MOCK_CONFIG};

mod common;
use common::*;

/// The project's own suite: a script run over everything.
const SUITE: &str = "sh suite.sh --all";

const PLAN_THEN_LOOP: &str = r#"
name: planned
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

/// A run whose planner hands over one task held to `criterion`, in a
/// repository holding the suite's script, under a config that measures it.
async fn planned(criterion: &str) -> Bench {
    let bench = Bench::new();
    tokio::fs::write(bench.worktree.join("suite.sh"), "exit 0\n")
        .await
        .unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "suite"]);
    let config = format!("{MOCK_CONFIG}baseline:\n  suite: \"{SUITE}\"\n");
    let mut fixture = plan_session(&format!(
        "tasks:\n{}",
        task_yaml("task-a", "a", "a.txt", criterion)
    ));
    fixture.push_str(
        "  - effects:\n      - { path: a.txt, content: a }\n    outcome: { type: completed, summary: did-a }\n",
    );
    bench
        .run_with_config(PLAN_THEN_LOOP, &fixture, &config)
        .await;
    bench
}

/// The codes every refused plan was refused with, in order.
fn refused(bench: &Bench) -> Vec<String> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Artifacts(ArtifactEvent::Submitted(submitted))) => {
                match &submitted.outcome {
                    SubmissionOutcome::Refused { report } => Some(report.diagnostics.clone()),
                    SubmissionOutcome::Accepted { .. } => None,
                }
            }
            _ => None,
        })
        .flatten()
        .map(|diagnostic| diagnostic.problem.code().to_string())
        .collect()
}

#[tokio::test]
async fn a_plan_that_runs_the_tests_outside_the_suite_is_refused_naming_it() {
    let bench = planned("sh suite.sh a.txt").await;

    assert!(
        refused(&bench).contains(&"outside-the-suite".to_string()),
        "{:?}",
        refused(&bench)
    );
}

#[tokio::test]
async fn the_planner_is_told_the_suite_it_extends_and_a_plan_that_does_is_taken() {
    let bench = planned("sh suite.sh --all a.txt && test -f a.txt").await;

    assert!(
        !refused(&bench).contains(&"outside-the-suite".to_string()),
        "{:?}",
        refused(&bench)
    );

    let requests = bench.mock().requests_seen();
    let planner = requests
        .iter()
        .find(|request| request.prompt.contains("Hand over the tasks document."))
        .expect("the planner's session");
    assert!(
        planner
            .prompt
            .contains("This run measures its suite with `sh suite.sh --all`"),
        "{}",
        planner.prompt
    );
}
