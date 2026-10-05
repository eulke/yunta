//! A scope request owed a person's decision while nobody is there to
//! answer it. Its task waits, the loop goes on with what does not depend
//! on it, and the loop ends owing the decision: the person grants it from
//! that failure — and the task picks its work back up with the wider scope
//! — or answers anything else, which denies it with a finding.

use yunta_core::events::{EventPayload, Failure, NodeEvent, ScopeEvent, TaskStatus};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{Bench, ScriptedInteraction};

mod common;
use common::*;

/// A loop whose task `task-h` asks for `b.txt` beside what it may write,
/// and whose second session writes both, as a granted scope allows.
fn asking(tasks: &[&str]) -> (String, String) {
    asking_then(tasks, "      - { path: b.txt, content: \"b\" }\n")
}

/// The same, with the second session writing `a.txt` and `more`.
fn asking_then(tasks: &[&str], more: &str) -> (String, String) {
    let workflow = scope_expansion_workflow("ask", &[], None);
    let declared: String = tasks.iter().copied().collect();
    let mut fixture = plan_session(&format!("tasks:\n{declared}"));
    fixture.push_str(&requesting_session("task-h"));
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-h\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n{more}    outcome: {{ type: completed, summary: did-h }}\n"
    ));
    (workflow, fixture)
}

fn chosen(option: &str) -> ScriptedInteraction {
    ScriptedInteraction::new(yunta_core::events::HumanChoice {
        option: option.into(),
        by: "lead".into(),
        free_text: None,
    })
}

/// What the loop node last failed on.
fn loop_failure(bench: &Bench) -> Option<Failure> {
    bench
        .events()
        .iter()
        .rev()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(failed))) => Some(failed.failure.clone()),
            _ => None,
        })
}

#[tokio::test]
async fn headless_ask_request_fails_the_loop_owing_it_and_a_grant_answers_it() {
    let bench = Bench::new();
    let (workflow, fixture) = asking(&[&task_yaml("task-h", "h", "a.txt", "test -f a.txt")]);
    let RunReport { terminal, .. } = bench.run(&workflow, &fixture).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let owed = loop_failure(&bench);
    assert!(matches!(owed, Some(Failure::ScopeOwed { .. })), "{owed:?}");

    let RunReport { terminal, state } = bench.wake_answering(&chosen("grant")).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("task-h"), Some(TaskStatus::Done));
    let granted = bench.events().into_iter().any(|event| {
        matches!(event.payload(), Some(EventPayload::Scope(ScopeEvent::Granted(p)))
            if p.task_id.as_ref().is_some_and(|task| task.as_str() == "task-h"))
    });
    assert!(granted, "the person's grant is on the log for the task");
}

#[tokio::test]
async fn retry_denies_with_a_finding() {
    let bench = Bench::new();
    let task = task_yaml("task-h", "h", "a.txt", "test -f a.txt");
    let (workflow, fixture) = asking_then(&[&task], "");
    bench.run(&workflow, &fixture).await;

    bench.wake_answering(&chosen("retry")).await;

    let events = bench.events();
    let denied = events.iter().any(|event| {
        matches!(event.payload(), Some(EventPayload::Scope(ScopeEvent::Denied(p)))
            if p.task_id.as_str() == "task-h")
    });
    assert!(denied, "any answer but a grant denies the request");
    let finding = findings_posted(&events)
        .into_iter()
        .any(|finding| finding.detail.contains("adjacent fix in b.txt"));
    assert!(finding, "and the denial becomes a finding");
}

#[tokio::test]
async fn an_owed_task_does_not_stop_an_independent_one() {
    let bench = Bench::new();
    let (workflow, mut fixture) = asking(&[
        &task_yaml("task-h", "h", "a.txt", "test -f a.txt"),
        &task_yaml("task-i", "i", "i.txt", "test -f i.txt"),
    ]);
    fixture.push_str(
        "  - match_prompt_contains: \"task-i\"\n    effects:\n      - { path: i.txt, content: \"i\" }\n    outcome: { type: completed, summary: did-i }\n",
    );

    let RunReport { terminal, state } = bench.run(&workflow, &fixture).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(state.tasks.status("task-i"), Some(TaskStatus::Done));
    assert!(matches!(
        loop_failure(&bench),
        Some(Failure::ScopeOwed { .. })
    ));
}
