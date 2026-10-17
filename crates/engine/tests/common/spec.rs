//! A planner, the node that specifies its tasks, and the loop that
//! builds them: the run more than one spec test drives.

use yunta_core::events::{ArtifactEvent, EventPayload, SubmissionOutcome};
use yunta_core::ArtifactKind;
use yunta_testkit::Bench;

use super::plan_session;

/// A plan of one task that writes `greeting.txt`.
pub const PLAN: &str = "\
tasks:
  - id: greet
    title: \"Write the greeting\"
    scope: [\"greeting.txt\"]
    criteria:
      - cmd: \"test -f greeting.txt\"
";

/// A planner, then the node that specifies its tasks.
pub const WORKFLOW: &str = r#"
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
pub fn spec_of(task: &str, test: &str, cmd: &str) -> String {
    format!(
        "{{ specs: [{{ task: {task}, files: [{{ path: tests/greet.sh, content: {test:?} }}], \
         tests: [{{ cmd: {cmd:?}, proves: \"the greeting says hello\" }}] }}] }}"
    )
}

/// The spec session: each document in turn, the last one accepted.
pub fn specifying(documents: &[(&str, bool)]) -> String {
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
pub fn refused(bench: &Bench) -> Vec<Vec<String>> {
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
pub const GREETS: &str = "test \"$(cat greeting.txt 2>/dev/null)\" = Hello\n";

/// The planner, the spec that holds `greet` to [`GREETS`], and the loop
/// that builds it.
pub fn specified_loop() -> String {
    format!(
        "{WORKFLOW}  - id: implement
    kind: loop
    runner: executor
    depends_on: [spec]
    until: all_tasks_complete
    prompt: \"Implement your task.\"
"
    )
}

/// The planner and spec sessions, then the task session writing `effects`.
pub fn building(effects: &str) -> String {
    building_on(PLAN, effects)
}

/// [`building`], on `plan`.
pub fn building_on(plan: &str, effects: &str) -> String {
    let spec = spec_of("greet", GREETS, "sh tests/greet.sh");
    plan_session(plan)
        + &specifying(&[(&spec, true)])
        + &format!(
            "  - match_prompt_contains: \"Implement your task\"
    effects:
{effects}    outcome: {{ type: completed, summary: built }}
"
        )
}
