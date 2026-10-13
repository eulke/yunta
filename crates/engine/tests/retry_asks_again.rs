//! A person who chooses to run a node again after its failure says the
//! world it failed in may have changed: what failed is asked again, and
//! what passed stays passed — whether the choice is made at the run's own
//! console or from outside while it is parked.

use yunta_core::events::{
    CriterionResult, Escalation, EventPayload, NodeEvent, Precondition, Remedy, TaskStatus,
};
use yunta_core::reserved::ReservedOption;
use yunta_engine::{current_escalation, RunReport, RunTerminal};
use yunta_testkit::Bench;

mod common;
use common::*;

const WORKFLOW: &str = "name: retried\ninputs:\n  tasks: { type: document, kind: tasks }\nnodes:\n  - id: implement\n    kind: loop\n    runner: executor\n    until: all_tasks_complete\n    prompt: \"Do your task.\"\n";

const FIXTURE: &str = "dialect: claude-code\nsessions:\n  - match_prompt_contains: \"task-h\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-h }\n";

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

/// The menu a parked run rebuilds from its log, as a wake rebuilds it.
async fn parked_menu(bench: &Bench) -> Escalation {
    let (_, escalation) = current_escalation(
        &bench.manifest(),
        &bench.run_dir(),
        &yunta_engine::derive(&bench.events()),
    )
    .await
    .unwrap()
    .expect("a red guard is a pause with a menu");
    escalation
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

/// A red guard waits for an input outside the tree, so its retry waits for
/// that input too: the menu a parked run rebuilds offers retry, and the
/// failure menu's retry remedy names the outside change it waits for.
#[tokio::test]
async fn a_red_guard_offers_a_retry_that_waits_for_something_outside_the_tree() {
    let outside = tempfile::tempdir().unwrap();
    let ready = outside.path().join("ready");
    let bench = parked_on_a_red_guard(&ready).await;

    let escalation = parked_menu(&bench).await;

    let offered: Vec<String> = escalation
        .options()
        .iter()
        .map(|option| option.id.to_string())
        .collect();
    assert_eq!(offered, ["retry", "abort"]);
    let failure = escalation
        .failure
        .as_ref()
        .expect("a failed node's menu names its failure");
    assert_eq!(
        failure.remedies,
        [Remedy {
            option: ReservedOption::Retry,
            precondition: Some(Precondition::OutsideTreeChanged),
        }]
    );
}
