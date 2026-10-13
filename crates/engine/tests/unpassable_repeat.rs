//! A criterion no work on the plan can pass is declared, the planner
//! rewrites the plan, and when the rewrite keeps the same criterion the
//! declaration repeats. A repeat is the same failure again: the run pauses
//! for a person rather than routing the planner a second time.

mod common;

use common::*;
use yunta_core::events::{EventPayload, Failure, SessionEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

/// One reroute back to the planner, the same workflow the declaration
/// tests route through.
const REPEAT_WORKFLOW: &str = r#"
name: departs
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
    prompt: "Implement your task."
    on_failure: { goto: plan, max_reroutes: 1 }
"#;

#[tokio::test]
async fn a_criterion_declared_again_after_the_rewrite_pauses_for_a_person() {
    let bench = Bench::new();
    let tasks = format!("tasks:\n{}", task_yaml("task-u", "u", "u.txt", "false"));
    let declare = "  - match_prompt_contains: \"`task-u`\"
    steps:
      - type: run_tool
        tool: yunta_declare_deviation
        arguments: { kind: unpassable, criterion: \"false\", why: \"false exits 1\" }
    outcome: { type: completed, summary: declared }
";
    // The planner keeps the impossible criterion and adds one that is red
    // before the work: the changed criteria re-run the task, and its session
    // declares the same criterion again.
    let rewritten = "tasks:
  - id: task-u
    title: \"u\"
    scope: [\"u.txt\"]
    criteria:
      - cmd: \"false\"
      - cmd: \"test -f u.txt\"
";
    let fixture = plan_session(&tasks) + declare + &tasks_session(rewritten, "replanned") + declare;

    let RunReport { terminal, .. } = bench.run(REPEAT_WORKFLOW, &fixture).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "a repeat pauses the run: {terminal:?}"
    );
    let events = bench.events();
    let planner_sessions = events
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Session(SessionEvent::Opened(p))) if p.task_id.is_none()
            )
        })
        .count();
    assert_eq!(planner_sessions, 2, "one reroute, and no second one");
    let declared = events
        .iter()
        .filter(|event| {
            event.payload().map(|payload| payload.kind_name())
                == Some("criterion_declared_unpassable")
        })
        .count();
    assert_eq!(declared, 2, "each session declares it once");
    let failure = loop_failure(&bench);
    assert!(
        matches!(failure, Some(Failure::Unchanged { .. })),
        "the repeat is the same failure again: {failure:?}"
    );
}
