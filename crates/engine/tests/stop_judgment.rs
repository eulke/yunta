//! A session that asks to stop with work its close would not take is sent
//! back to it, once for one state of that work, with what stands in the
//! way — so the same session fixes it instead of closing, failing and
//! handing the work to another.

use yunta_core::events::{
    EventPayload, NodeEvent, Phase, SessionEvent, StopRefusal, StopRefusedPayload, TaskStatus,
};
use yunta_engine::{NodeState, RunReport, RunTerminal};
use yunta_testkit::Bench;

mod common;
use common::*;

/// Every stop the run sent back, in order.
fn stops_refused(bench: &Bench) -> Vec<StopRefusedPayload> {
    bench
        .events()
        .into_iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Session(SessionEvent::StopRefused(refused))) => {
                Some(refused.clone())
            }
            _ => None,
        })
        .collect()
}

/// The loop's fixture: the planner hands over `task-a`, which must write
/// `a.txt`, and the sessions after it follow in dispatch order.
fn fixture(sessions: &str) -> String {
    let mut fixture = plan_session(&format!(
        "tasks:\n{}",
        task_yaml("task-a", "a", "a.txt", "test -f a.txt")
    ))
    .replace(
        "capabilities: { run_tools: true }",
        "capabilities: { run_tools: true, resume_session: true, fence: tool_calls }",
    );
    fixture.push_str(sessions);
    fixture
}

#[tokio::test]
async fn a_red_stop_is_sent_back_once_and_the_same_session_closes_its_task() {
    let bench = Bench::new();
    let fixture = fixture(
        "  - match_prompt_contains: \"task-a\"\n    before_stop:\n      writes:\n        - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-a }\n",
    );

    let RunReport { terminal, state } = bench
        .run(&scope_expansion_workflow("deny", &[], None), &fixture)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("task-a"), Some(TaskStatus::Done));
    let refused = stops_refused(&bench);
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].why, vec![StopRefusal::CriteriaRed]);
    assert_eq!(
        refused[0].task_id.as_ref().map(|t| t.as_str()),
        Some("task-a")
    );
    assert_eq!(
        bench.mock().requests_seen().len(),
        2,
        "the planner and one task session: the work never went to another"
    );
    let closing = bench
        .events()
        .into_iter()
        .rev()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::CriteriaChecked(checked)))
                if checked.phase == Phase::Post =>
            {
                Some(checked.clone())
            }
            _ => None,
        })
        .expect("the close checked the task");
    assert!(
        closing.results.iter().all(|result| result.reused),
        "the close reads what the stop's judgement ran: {closing:?}"
    );
}

#[tokio::test]
async fn a_stop_on_the_same_work_is_sent_back_only_once() {
    let bench = Bench::new();
    let fixture = fixture(
        "  - match_prompt_contains: \"task-a\"\n    before_stop: {}\n    outcome: { type: completed, summary: gave-up }\n",
    );

    let RunReport { terminal, state } = bench
        .run(&scope_expansion_workflow("deny", &[], None), &fixture)
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(state.tasks.status("task-a"), Some(TaskStatus::Blocked));
    assert_eq!(
        stops_refused(&bench).len(),
        1,
        "the session that stopped again unchanged was let go, and its close judged it"
    );
}

#[tokio::test]
async fn a_session_that_asked_for_scope_is_never_sent_back() {
    let bench = Bench::new();
    tokio::fs::write(bench.worktree.join("check.sh"), "test -f b.txt\n")
        .await
        .unwrap();
    yunta_testkit::git(&bench.worktree, &["add", "-A"]);
    yunta_testkit::git(&bench.worktree, &["commit", "-q", "-m", "check"]);
    let mut fixture = plan_session(&format!(
        "tasks:\n{}",
        task_yaml("task-b", "b", "a.txt", "sh check.sh")
    ))
    .replace(
        "capabilities: { run_tools: true }",
        "capabilities: { run_tools: true, resume_session: true }",
    );
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-b\"\n    effects:\n      - {{ path: {:?}, content: \"paths: [b.txt]\\nreason: the check reads b.txt\\nproposed_criterion:\\n  cmd: test -f b.txt\\n\" }}\n    before_stop: {{}}\n    outcome: {{ type: completed, summary: asked }}\n\
         \x20 - match_prompt_contains: \"was granted\"\n    effects:\n      - {{ path: b.txt, content: \"b\" }}\n    outcome: {{ type: completed, summary: did-b }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE
    ));

    let RunReport { terminal, state } = bench
        .run(
            &scope_expansion_workflow("rules", &["b.txt"], None),
            &fixture,
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("task-b"), Some(TaskStatus::Done));
    assert!(
        stops_refused(&bench).is_empty(),
        "the attempt's end owed it the answer"
    );
}

#[tokio::test]
async fn a_node_session_without_its_document_is_sent_back_to_hand_it_over() {
    let bench = Bench::new();
    let workflow = "name: w\nnodes:\n  - id: plan\n    kind: prompt\n    runner: planner\n    prompt: \"Hand over the tasks document.\"\n    artifacts:\n      produces: [tasks]\n";
    let handed = tasks_session(
        &format!(
            "tasks:\n{}",
            task_yaml("task-a", "a", "a.txt", "test -f a.txt")
        ),
        "planned",
    );
    // The planner's script, with its submission moved after the refusal.
    let steps: String = handed
        .split_once("  - steps:\n")
        .and_then(|(_, rest)| rest.split_once("    outcome:"))
        .map(|(steps, _)| steps.lines().map(|line| format!("  {line}\n")).collect())
        .unwrap();
    let fixture = format!(
        "capabilities: {{ run_tools: true }}\nsessions:\n  - before_stop:\n      steps:\n{steps}    outcome: {{ type: completed, summary: planned }}\n"
    );

    let RunReport { terminal, state } = bench.run(workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished, "{fixture}");
    assert!(matches!(
        state.nodes.state("plan"),
        Some(NodeState::Finished { .. })
    ));
    let refused = stops_refused(&bench);
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].why, vec![StopRefusal::ArtifactMissing]);
    assert_eq!(refused[0].task_id, None);
}

#[tokio::test]
async fn a_session_whose_refused_write_its_red_output_locates_is_never_sent_back() {
    let bench = Bench::new();
    let check = "test -f b.rs && exit 0\necho '  --> b.rs:3:9'\nexit 1\n";
    tokio::fs::write(bench.worktree.join("check.sh"), check)
        .await
        .unwrap();
    yunta_testkit::git(&bench.worktree, &["add", "-A"]);
    yunta_testkit::git(&bench.worktree, &["commit", "-q", "-m", "check"]);
    let mut fixture = plan_session(&format!(
        "tasks:\n{}",
        task_yaml("task-h", "h", "a.rs", "sh check.sh")
    ))
    .replace(
        "capabilities: { run_tools: true }",
        "capabilities: { run_tools: true, resume_session: true, fence: tool_calls }",
    );
    fixture.push_str(
        "  - match_prompt_contains: \"task-h\"\n    effects:\n      - { path: a.rs, content: \"a\" }\n      - { path: b.rs, content: \"b\" }\n    before_stop: {}\n    outcome: { type: completed, summary: did-h }\n\
         \x20 - match_prompt_contains: \"was granted\"\n    effects:\n      - { path: b.rs, content: \"b\" }\n    outcome: { type: completed, summary: did-b }\n",
    );

    let RunReport { terminal, state } = bench
        .run(&scope_expansion_workflow("ask", &[], None), &fixture)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("task-h"), Some(TaskStatus::Done));
    assert!(
        stops_refused(&bench).is_empty(),
        "the attempt's end grants the write by its evidence and resumes the session"
    );
}
