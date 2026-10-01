//! The tests a person approved are denied to every session of the run,
//! not only to the task they hold: a node after the loop, a node scoped
//! to what the run changed, another task. Each is refused the files as
//! it writes and again as it closes, and none of their changes lands.

mod common;

use std::path::PathBuf;

use common::spec::*;
use yunta_core::events::{EventPayload, Failure, NodeEvent, StoredEvent, TaskStatus};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git, git_output, write, Bench, MOCK_CONFIG};

/// Each failure of `node`.
fn failures(events: &[StoredEvent], node: &str) -> Vec<Failure> {
    events
        .iter()
        .filter(|event| event.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(p))) => Some(p.failure.clone()),
            _ => None,
        })
        .collect()
}

/// Every path a scope audit of the run found denied.
fn denied(events: &[StoredEvent]) -> Vec<PathBuf> {
    events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::ScopeChecked(p))) => Some(p.denied.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn the_test() -> PathBuf {
    PathBuf::from("tests/greet.sh")
}

/// The run's branch holds `path` as `content`.
fn at_head(bench: &Bench, path: &str) -> String {
    git_output(&bench.worktree, &["show", &format!("HEAD:{path}")])
}

/// The specified loop, then a node that tries to make the test pass by
/// rewriting it, scoped to what the run changed.
fn then_a_fix() -> String {
    format!(
        "{}  - id: fix
    kind: prompt
    runner: executor
    depends_on: [implement]
    scope: run
    prompt: \"Fix the lint errors.\"
",
        specified_loop()
    )
}

/// The fixture that builds the greeting, then a fix that rewrites its
/// test.
fn fixing() -> String {
    building("      - { path: greeting.txt, content: Hello }\n")
        + "  - match_prompt_contains: \"Fix the lint\"
    effects:
      - { path: tests/greet.sh, content: \"exit 0\\n\" }
    outcome: { type: completed, summary: fixed }
"
}

#[tokio::test]
async fn a_node_after_the_loop_that_changes_an_approved_test_fails_and_its_change_never_lands() {
    let workflow = format!(
        "{}  - id: tamper
    kind: bash
    depends_on: [implement]
    run: \"printf 'exit 0\\\\n' > tests/greet.sh\"
",
        specified_loop()
    );
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run(
            &workflow,
            &building("      - { path: greeting.txt, content: Hello }\n"),
        )
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(
        failures(&bench.events(), "tamper"),
        vec![Failure::paths_denied(vec![the_test()])]
    );
    assert_eq!(at_head(&bench, "tests/greet.sh"), GREETS.trim_end());
    let left = tokio::fs::read_to_string(bench.worktree.join("tests/greet.sh"))
        .await
        .unwrap();
    assert_eq!(left, GREETS, "put back as the branch has it");
}

#[tokio::test]
async fn a_node_scoped_to_the_run_is_refused_an_approved_test_without_a_grant() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(&then_a_fix(), &fixing()).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(
        failures(&bench.events(), "fix"),
        vec![Failure::paths_denied(vec![the_test()])],
        "a test a person approved is never put to a person as a grant"
    );
    assert_eq!(at_head(&bench, "tests/greet.sh"), GREETS.trim_end());
}

#[tokio::test]
async fn every_session_after_the_spec_is_accepted_is_fenced_from_every_test() {
    let bench = Bench::new();
    bench.run(&then_a_fix(), &fixing()).await;

    // The planner, the spec writer, the task, the fix.
    let requests = bench.mock().requests_seen();
    assert_eq!(requests.len(), 4, "{requests:?}");
    let glob: yunta_core::ScopeGlob = "tests/greet.sh".into();
    for (session, request) in requests.iter().enumerate().skip(2) {
        assert!(
            request.fence.denied.contains(&glob),
            "session {session} is fenced from the test: {:?}",
            request.fence.denied
        );
    }
}

/// Two tasks, each held to a test of its own; the second may write
/// anywhere under `tests/`.
const TWO_TASKS: &str = "\
tasks:
  - id: greet
    title: \"Write the greeting\"
    scope: [\"greeting.txt\"]
    criteria:
      - cmd: \"test -f greeting.txt\"
  - id: farewell
    title: \"Write the farewell\"
    scope: [\"farewell.txt\", \"tests/**\"]
    depends_on: [greet]
    criteria:
      - cmd: \"test -f farewell.txt\"
";

#[tokio::test]
async fn a_task_that_rewrites_another_tasks_test_is_not_integrated() {
    let spec = "{ specs: [\
        { task: greet, files: [{ path: tests/greet.sh, content: \"test \\\"$(cat greeting.txt 2>/dev/null)\\\" = Hello\\n\" }], \
          tests: [{ cmd: \"sh tests/greet.sh\", proves: \"the greeting says hello\" }] }, \
        { task: farewell, files: [{ path: tests/farewell.sh, content: \"test -f farewell.txt\\n\" }], \
          tests: [{ cmd: \"sh tests/farewell.sh\", proves: \"there is a farewell\" }] }] }";
    let fixture = common::plan_session(TWO_TASKS)
        + &specifying(&[(spec, true)])
        + "  - match_prompt_contains: \"Implement your task\"
    effects:
      - { path: greeting.txt, content: Hello }
    outcome: { type: completed, summary: greeted }
  - match_prompt_contains: \"Implement your task\"
    effects:
      - { path: farewell.txt, content: Bye }
      - { path: tests/greet.sh, content: \"exit 0\\n\" }
    outcome: { type: completed, summary: said farewell }
";
    let bench = Bench::new();
    let RunReport { state, .. } = bench.run(&specified_loop(), &fixture).await;

    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Done));
    assert_eq!(state.tasks.status("farewell"), Some(TaskStatus::Blocked));
    assert!(denied(&bench.events()).contains(&the_test()));
    assert_eq!(at_head(&bench, "tests/greet.sh"), GREETS.trim_end());
}

#[tokio::test]
async fn in_a_run_without_its_own_worktree_the_loop_that_lays_its_tests_is_not_refused_for_them() {
    let config = format!("{MOCK_CONFIG}defaults:\n  isolation: none\n");
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run_with_config(
            &specified_loop(),
            &building("      - { path: greeting.txt, content: Hello }\n"),
            &config,
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Done));
}

#[tokio::test]
async fn a_spec_that_names_a_file_the_run_already_holds_is_refused_saying_which() {
    let bench = Bench::new();
    write(
        &bench.worktree.join("tests/greet.sh"),
        "echo the project's own\n",
    );
    git(&bench.worktree, &["add", "tests/greet.sh"]);
    git(
        &bench.worktree,
        &["commit", "-q", "-m", "a test of the project's"],
    );
    let over = spec_of("greet", GREETS, "sh tests/greet.sh");
    let beside = format!(
        "{{ specs: [{{ task: greet, files: [{{ path: tests/greeting.sh, content: {GREETS:?} }}], \
         tests: [{{ cmd: \"sh tests/greeting.sh\", proves: \"the greeting says hello\" }}] }}] }}"
    );
    let fixture = common::plan_session(PLAN) + &specifying(&[(&over, false), (&beside, true)]);
    let RunReport { terminal, .. } = bench.run(WORKFLOW, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(refused(&bench), [vec!["test-file-exists".to_string()]]);
    assert_eq!(at_head(&bench, "tests/greet.sh"), "echo the project's own");
}
