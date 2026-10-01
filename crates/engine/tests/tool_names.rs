//! A session is shown the run's tools under the names its CLI gives
//! them: every text the engine writes for it — the shape of a document
//! it hands over, what it is told about its node and its task, and the
//! names any other text may use — calls a tool what the session can call.

mod common;

use common::*;
use yunta_core::events::{EventPayload, NodeEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

const WORKFLOW: &str = r#"
name: named
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
"#;

/// The planner and the one task's session, under a CLI that names a
/// server's tools `mcp__<server>__<tool>`.
fn fixture() -> String {
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("greet", "greet", "a.txt", "test -f a.txt")
    );
    plan_session(&tasks).replace(
        "capabilities: { run_tools: true }",
        "capabilities: { run_tools: true, tool_naming: mcp_prefixed }",
    ) + "  - match_prompt_contains: \"Implement your task\"
    effects:
      - { path: a.txt, content: a }
    outcome: { type: completed, summary: built }
"
}

#[tokio::test]
async fn every_text_a_session_is_shown_names_a_run_tool_the_way_its_cli_does() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(WORKFLOW, &fixture()).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let prompts: Vec<String> = bench
        .mock()
        .requests_seen()
        .into_iter()
        .map(|request| request.prompt)
        .collect();
    let [planner, task] = prompts.as_slice() else {
        panic!("two sessions: {prompts:#?}");
    };
    for wanted in [
        "hand it over with `mcp__yunta-run__yunta_submit_tasks`",
        "the `tasks` document → `mcp__yunta-run__yunta_submit_tasks`",
        "`mcp__yunta-run__yunta_submit_tasks` — yunta_submit_tasks",
    ] {
        assert!(planner.contains(wanted), "`{wanted}` in:\n{planner}");
    }
    for wanted in [
        "with `mcp__yunta-run__yunta_task` before you change anything",
        "`mcp__yunta-run__yunta_check_task` judges your work",
        "`mcp__yunta-run__yunta_task` — yunta_task",
    ] {
        assert!(task.contains(wanted), "`{wanted}` in:\n{task}");
    }
    for prompt in [planner, task] {
        for bare in ["`yunta_submit_tasks`", "`yunta_task`", "`yunta_check_task`"] {
            assert!(!prompt.contains(bare), "{bare} in:\n{prompt}");
        }
    }
}

#[tokio::test]
async fn a_node_s_runner_is_resolved_before_its_context_names_any_tool() {
    let bench = Bench::new();
    bench.run(WORKFLOW, &fixture()).await;
    let events = bench.events();
    let order: Vec<&str> = events
        .iter()
        .filter(|event| {
            event
                .node_id
                .as_ref()
                .is_some_and(|node| node.as_str() == "plan")
        })
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::RunnerResolved(_))) => Some("runner"),
            Some(EventPayload::Node(NodeEvent::ContextAssembled(_))) => Some("context"),
            _ => None,
        })
        .collect();
    assert_eq!(order, ["runner", "context"]);
}
