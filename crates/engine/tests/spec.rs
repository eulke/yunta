//! A spec: the tests a plan's tasks are held to, written before any task
//! is built and by someone other than whoever builds it.
//!
//! Handed over, it is held to the plan the run holds and proven where
//! its tests run: every task it names is the plan's, and with every one
//! of its files written into the run's tree, each test runs and fails.

mod common;

use common::*;
use yunta_core::events::{ArtifactEvent, EventPayload, SubmissionOutcome};
use yunta_core::ArtifactKind;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

/// A plan of one task that writes `greeting.txt`.
const PLAN: &str = "\
tasks:
  - id: greet
    title: \"Write the greeting\"
    scope: [\"greeting.txt\"]
    criteria:
      - cmd: \"test -f greeting.txt\"
";

/// A planner, then the node that specifies its tasks.
const WORKFLOW: &str = r#"
name: specified
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: spec
    kind: prompt
    runner: planner
    permissions: read-only
    depends_on: [plan]
    prompt: "Write the tests the plan's tasks are held to."
    artifacts:
      produces: [spec]
"#;

/// The spec of `task`: one file, `tests/greet.sh`, holding `test`, run
/// by `cmd`.
fn spec_of(task: &str, test: &str, cmd: &str) -> String {
    format!(
        "{{ specs: [{{ task: {task}, files: [{{ path: tests/greet.sh, content: {test:?} }}], \
         tests: [{{ cmd: {cmd:?}, proves: \"the greeting says hello\" }}] }}] }}"
    )
}

/// The spec session: each document in turn, the last one accepted.
fn specifying(documents: &[(&str, bool)]) -> String {
    let steps: String = documents
        .iter()
        .map(|(document, accepted)| {
            let expect = if *accepted {
                ""
            } else {
                "        expect: refused\n"
            };
            format!(
                "      - type: run_tool\n        tool: yunta_submit_spec\n{expect}\
                 \x20       arguments:\n          document: {document}\n"
            )
        })
        .collect();
    format!(
        "  - match_prompt_contains: \"Write the tests\"\n    steps:\n{steps}\
         \x20   outcome: {{ type: completed, summary: specified }}\n"
    )
}

/// The codes of every spec the run refused, in submission order.
fn refused(bench: &Bench) -> Vec<Vec<String>> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Artifacts(ArtifactEvent::Submitted(p)))
                if p.artifact_kind == ArtifactKind::Spec =>
            {
                match &p.outcome {
                    SubmissionOutcome::Refused { report } => Some(
                        report
                            .diagnostics
                            .iter()
                            .map(|d| d.code().as_str().to_string())
                            .collect(),
                    ),
                    SubmissionOutcome::Accepted { .. } => None,
                }
            }
            _ => None,
        })
        .collect()
}

/// A test that fails until `greeting.txt` says hello — and only runs
/// when its own file is in the tree.
const GREETS: &str = "test \"$(cat greeting.txt 2>/dev/null)\" = Hello\n";

#[tokio::test]
async fn a_spec_whose_tests_fail_before_the_work_is_accepted_with_its_files_in_the_tree() {
    let spec = spec_of("greet", GREETS, "sh tests/greet.sh");
    let fixture = plan_session(PLAN) + &specifying(&[(&spec, true)]);
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(WORKFLOW, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(refused(&bench).is_empty(), "{:?}", refused(&bench));
}

#[tokio::test]
async fn a_spec_whose_test_already_passes_or_names_no_task_of_the_plan_is_refused() {
    let passes = spec_of("greet", "exit 0\n", "sh tests/greet.sh");
    let strays = spec_of("farewell", GREETS, "sh tests/greet.sh");
    let holds = spec_of("greet", GREETS, "sh tests/greet.sh");
    let fixture =
        plan_session(PLAN) + &specifying(&[(&passes, false), (&strays, false), (&holds, true)]);
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(WORKFLOW, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        refused(&bench),
        [
            vec!["criterion-already-passes".to_string()],
            vec!["unknown-spec-task".to_string()],
        ]
    );
}
