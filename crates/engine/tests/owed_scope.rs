//! A scope request owed a person's decision while nobody is there to
//! answer it. Its task waits, the loop goes on with what does not depend
//! on it, and the loop ends owing the decision: the person grants it from
//! that failure — and the task picks its work back up with the wider scope
//! — or answers anything else, which denies it with a finding. A request
//! its task leaves unanswered when the task closes is withdrawn instead:
//! nobody decides it, and the loop owes nothing.

use yunta_core::events::{EventPayload, Failure, ScopeEvent, SessionEvent, TaskEvent, TaskStatus};
use yunta_core::SessionId;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{Bench, ScriptedInteraction};

mod common;
use common::*;

/// A loop whose task `task-h` asks for `b.txt` beside what it may write, and
/// whose fresh session writes nothing, so the request stays pending and owed.
fn asking(tasks: &[&str]) -> (String, String) {
    asking_then(tasks, "")
}

/// The same, with the fresh session the pending request opens writing `more`.
/// A session resumed after a grant writes `b.txt`, as the granted scope allows.
fn asking_then(tasks: &[&str], more: &str) -> (String, String) {
    let workflow = scope_expansion_workflow("ask", None);
    let declared: String = tasks.iter().copied().collect();
    let mut fixture = plan_session(&format!("tasks:\n{declared}"));
    fixture.push_str(&requesting_session("task-h"));
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-h\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n{more}    outcome: {{ type: completed, summary: did-h }}\n"
    ));
    fixture.push_str(
        "  - match_prompt_contains: \"task-h\"\n    effects:\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: did-b }\n",
    );
    // Sessions a refusal opens to re-run the task inside its own scope.
    for _ in 0..3 {
        fixture.push_str(
            "  - match_prompt_contains: \"task-h\"\n    outcome: { type: completed, summary: retried }\n",
        );
    }
    (workflow, fixture)
}

/// A task that needs both files: the request's b.txt is what keeps it open.
fn needing_both(id: &str, title: &str) -> String {
    task_yaml(id, title, "a.txt", "test -f a.txt && test -f b.txt")
}

fn chosen(option: &str) -> ScriptedInteraction {
    ScriptedInteraction::new(yunta_core::events::HumanChoice {
        option: option.into(),
        by: "lead".into(),
        free_text: None,
    })
}

#[tokio::test]
async fn headless_ask_request_fails_the_loop_owing_it_and_a_grant_answers_it() {
    let bench = Bench::new();
    let (workflow, fixture) = asking(&[&needing_both("task-h", "h")]);
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
    // The first attempt asks for b.txt and writes nothing, so its criterion
    // stays red and the request is owed. Denied, the task re-runs inside its
    // scope, writes what its criterion needs, and closes.
    let mut fixture = plan_session(&format!(
        "tasks:\n{}",
        task_yaml("task-h", "h", "a.txt", "test -f a.txt")
    ));
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-h\"\n{}    outcome: {{ type: completed, summary: asked }}\n",
        request_step(serde_json::json!({
            "paths": ["b.txt"],
            "reason": "adjacent fix in b.txt",
            "proposed_criterion": { "cmd": "test -f nonexistent-marker" },
        })),
    ));
    fixture.push_str(
        "  - match_prompt_contains: \"task-h\"\n    outcome: { type: completed, summary: fresh }\n",
    );
    fixture.push_str(
        "  - match_prompt_contains: \"task-h\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-h }\n",
    );
    let workflow = scope_expansion_workflow("ask", None);
    bench.run(&workflow, &fixture).await;

    bench.wake_answering(&chosen("deny")).await;

    let events = bench.events();
    let denied = events.iter().any(|event| {
        matches!(event.payload(), Some(EventPayload::Scope(ScopeEvent::Denied(p)))
            if p.task_id.as_ref().is_some_and(|task| task.as_str() == "task-h"))
    });
    assert!(denied, "any answer but a grant denies the request");
    let finding = findings_posted(&events)
        .into_iter()
        .any(|finding| finding.detail.contains("adjacent fix in b.txt"));
    assert!(finding, "and the denial becomes a finding");
}

#[tokio::test]
async fn an_owed_grant_records_the_loop_mode_in_force() {
    let bench = Bench::new();
    // The fixture `asking` builds, run under `rules` with no grant to spend:
    // the request is owed at once, and the person's grant records that mode.
    let (_, fixture) = asking(&[&needing_both("task-h", "h")]);
    bench
        .run(&scope_expansion_workflow("rules", Some(0)), &fixture)
        .await;

    bench.wake_answering(&chosen("grant")).await;

    let granted = bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Scope(ScopeEvent::Granted(payload))) => Some(payload.clone()),
            _ => None,
        })
        .expect("a grant");
    assert_eq!(serde_json::to_value(&granted).unwrap()["mode"], "rules");
}

#[tokio::test]
async fn an_owed_task_does_not_stop_an_independent_one() {
    let bench = Bench::new();
    let (workflow, mut fixture) = asking(&[
        &needing_both("task-h", "h"),
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

#[tokio::test]
async fn a_task_that_closes_withdraws_the_request_it_leaves() {
    let bench = Bench::new();
    let task = task_yaml("task-h", "h", "a.txt", "test -f a.txt");
    let (workflow, fixture) = asking_then(&[&task], "");

    let RunReport { terminal, state } = bench.run(&workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("task-h"), Some(TaskStatus::Done));
    assert!(loop_failure(&bench).is_none());
    let withdrawn = bench
        .events()
        .iter()
        .filter(|event| {
            event.payload().map(|payload| payload.kind_name()) == Some("scope_expansion_withdrawn")
        })
        .count();
    assert_eq!(withdrawn, 1);
}

#[tokio::test]
async fn a_request_left_pending_when_the_session_fails_for_good_is_owed_a_person() {
    // The session asks for b.txt and then fails in a way no retry cures. The
    // cycle ends without closing, so the request is still a person's to decide.
    let bench = Bench::new();
    let task = task_yaml("task-h", "h", "a.txt", "test -f a.txt && test -f b.txt");
    let mut fixture = plan_session(&format!("tasks:\n{task}"));
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-h\"\n{}    outcome: {{ type: failed, message: \"Invalid API key\" }}\n",
        request_step(serde_json::json!({
            "paths": ["b.txt"],
            "reason": "adjacent fix in b.txt",
            "proposed_criterion": { "cmd": "test -f nonexistent-marker" },
        })),
    ));
    let RunReport { terminal, .. } = bench
        .run(&scope_expansion_workflow("ask", None), &fixture)
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let owed = loop_failure(&bench);
    assert!(matches!(owed, Some(Failure::ScopeOwed { .. })), "{owed:?}");
}

/// A request left pending under a red criterion opens one fresh session, which
/// may close the task without it. When that one does not close it either, the
/// request is owed a person, and no further session spends the run while they
/// decide, however much each session moved the work.
#[tokio::test]
async fn a_pending_request_opens_one_fresh_session_and_then_waits_for_a_person() {
    let bench = Bench::new();
    let mut fixture = plan_session(&format!("tasks:\n{}", needing_both("task-h", "h")));
    fixture.push_str(&requesting_session("task-h"));
    // Each session moves the work but the last, which leaves it as it found it.
    for content in ["a2", "a3", "a4", "a4"] {
        fixture.push_str(&format!(
            "  - match_prompt_contains: \"task-h\"\n    effects:\n      - {{ path: a.txt, content: \"{content}\" }}\n    outcome: {{ type: completed, summary: moved }}\n"
        ));
    }
    let RunReport { terminal, .. } = bench
        .run(&scope_expansion_workflow("ask", None), &fixture)
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let owed = loop_failure(&bench);
    assert!(matches!(owed, Some(Failure::ScopeOwed { .. })), "{owed:?}");
    let opened = bench
        .events()
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Session(SessionEvent::Opened(opened)))
                    if opened.task_id.as_ref().is_some_and(|id| id.as_str() == "task-h")
            )
        })
        .count();
    assert_eq!(opened, 2, "the session that asked, and one fresh session");
}

/// The id of the last session the log opened for `task`.
fn last_opened(bench: &Bench, task: &str) -> Option<SessionId> {
    bench
        .events()
        .into_iter()
        .rev()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Session(SessionEvent::Opened(opened)))
                if opened
                    .task_id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == task) =>
            {
                Some(opened.session_id.clone())
            }
            _ => None,
        })
}

/// A person's grant from the failure the loop ended owing sends the task
/// back to the session it worked in last, with the work it left: the
/// answer is the one thing that changed for that session.
#[tokio::test]
async fn an_owed_grant_resumes_the_session_the_task_worked_in_last() {
    let bench = Bench::new();
    let (workflow, fixture) = asking(&[&needing_both("task-h", "h")]);
    bench.run(&workflow, &fixture).await;
    let asked = last_opened(&bench, "task-h");
    assert!(
        asked.is_some(),
        "the task opened a session before the pause"
    );

    bench.wake_answering(&chosen("grant")).await;

    let resumed = bench
        .events()
        .into_iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Tasks(TaskEvent::StatusChanged(changed)))
                if changed.task_id.as_str() == "task-h" && changed.resumes.is_some() =>
            {
                changed.resumes.clone()
            }
            _ => None,
        });
    assert_eq!(resumed, asked);
}
