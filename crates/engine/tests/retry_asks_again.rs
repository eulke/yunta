//! A person who chooses to run a node again after its failure says the
//! world it failed in may have changed: what failed is asked again, and
//! what passed stays passed — whether the choice is made at the run's own
//! console or from outside while it is parked.

use yunta_core::events::{CriterionResult, EventPayload, NodeEvent, TaskStatus};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

mod common;
use common::*;

const WORKFLOW: &str = "name: retried\ninputs:\n  tasks: { type: document, kind: tasks }\nnodes:\n  - id: implement\n    kind: loop\n    runner: executor\n    until: all_tasks_complete\n    prompt: \"Do your task.\"\n";

const FIXTURE: &str = "capabilities: { run_tools: true }\nsessions:\n  - match_prompt_contains: \"task-h\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-h }\n";

/// A run whose one task is held to a guard that reads `ready`, outside
/// its tree, and to one that always passes; it parks with the first red.
/// It runs with an environment of its own, as a real one does, so a later
/// invocation takes in what this one answered.
async fn parked_on_a_red_guard(ready: &std::path::Path) -> Bench {
    let bench = Bench::new()
        .with_inputs(&[("tasks", "plan.yaml")])
        .with_subprocess_vars(Vec::new());
    let plan = format!(
        "tasks:\n  - id: task-h\n    title: h\n    scope: [a.txt]\n    criteria:\n      - cmd: \"test -f a.txt\"\n      - cmd: \"test -f {}\"\n        type: guard\n      - cmd: \"true\"\n        type: guard\n",
        ready.display()
    );
    tokio::fs::write(bench.worktree.join("plan.yaml"), plan)
        .await
        .unwrap();
    let RunReport { terminal, .. } = bench.run(WORKFLOW, FIXTURE).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    bench
}

/// What the last pre-check of `task-h` answered for `cmd`.
fn last_pre_check(bench: &Bench, cmd: &str) -> Option<CriterionResult> {
    bench
        .events()
        .iter()
        .rev()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::CriteriaChecked(check)))
                if check.phase == yunta_core::events::Phase::Pre =>
            {
                check
                    .results
                    .iter()
                    .find(|result| result.cmd == cmd)
                    .cloned()
            }
            _ => None,
        })
}

#[tokio::test]
async fn a_retry_at_the_console_reruns_the_guard_that_stopped_the_task() {
    let outside = tempfile::tempdir().unwrap();
    let ready = outside.path().join("ready");
    let bench = parked_on_a_red_guard(&ready).await;
    tokio::fs::write(&ready, "").await.unwrap();

    let retry = SequencedInteraction::choosing(&["retry"]);
    let RunReport { terminal, state } = bench.wake_answering(&retry).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("task-h"), Some(TaskStatus::Done));
}

#[tokio::test]
async fn a_retry_answered_while_parked_reruns_the_guard_on_the_next_wake() {
    let outside = tempfile::tempdir().unwrap();
    let ready = outside.path().join("ready");
    let bench = parked_on_a_red_guard(&ready).await;
    tokio::fs::write(&ready, "").await.unwrap();

    answer_parked(&bench, "retry").await.unwrap();
    let RunReport { terminal, state } = bench.wake().await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("task-h"), Some(TaskStatus::Done));
}

#[tokio::test]
async fn a_retry_keeps_what_passed() {
    let outside = tempfile::tempdir().unwrap();
    let ready = outside.path().join("ready");
    let bench = parked_on_a_red_guard(&ready).await;
    tokio::fs::write(&ready, "").await.unwrap();

    answer_parked(&bench, "retry").await.unwrap();
    bench.wake().await;

    let red_before = last_pre_check(&bench, &format!("test -f {}", ready.display()));
    let green_before = last_pre_check(&bench, "true");
    assert_eq!(
        red_before.map(|r| (r.exit_code, r.reused)),
        Some((0, false))
    );
    assert_eq!(
        green_before.map(|r| (r.exit_code, r.reused)),
        Some((0, true))
    );
}
