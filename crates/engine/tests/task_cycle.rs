use std::path::Path;

use yunta_adapters::MockAdapter;
use yunta_core::events::Criterion;
use yunta_core::Task;
use yunta_engine::{run_task, DispatchOutcome, PreCheckOutcome, TaskOutcome};

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join(".gitkeep"), "").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

fn cmd(cmd: &str) -> Criterion {
    Criterion {
        cmd: cmd.to_string(),
        r#type: None,
    }
}

fn guard(cmd: &str) -> Criterion {
    Criterion {
        cmd: cmd.to_string(),
        r#type: Some(yunta_core::events::CriterionType::Guard),
    }
}

fn task(id: &str, scope: &[&str], criteria: Vec<Criterion>) -> Task {
    Task {
        id: id.into(),
        title: "test task".to_string(),
        scope: scope.iter().map(|s| s.to_string()).collect(),
        criteria,
        depends_on: vec![],
        notes: None,
        manual_review: false,
        justification: None,
    }
}

#[tokio::test]
async fn a_session_that_makes_the_criterion_pass_reaches_done() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let t = task(
        "write-output",
        &["output.txt"],
        vec![cmd("test -f output.txt")],
    );
    let adapter = MockAdapter::from_yaml(
        r#"
effects:
  - { path: output.txt, content: "hello\n" }
outcome: { type: completed, summary: "wrote it" }
"#,
    )
    .unwrap();

    let report = run_task(&t, &adapter, dir.path(), 2).await.unwrap();

    assert_eq!(report.outcome, TaskOutcome::Done);
    assert_eq!(report.attempts.len(), 1);
    assert!(report.attempts[0].succeeded);
    assert_eq!(
        report.attempts[0].dispatch,
        DispatchOutcome::Completed {
            summary: "wrote it".to_string()
        }
    );
}

#[tokio::test]
async fn an_agent_that_claims_success_without_meeting_criteria_never_reaches_done() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let t = task(
        "write-output",
        &["output.txt"],
        vec![cmd("test -f output.txt")],
    );
    // The mock reports Completed but its fixture never writes output.txt —
    // this is exactly T5.2's acceptance criterion: the engine, not the
    // agent's self-report, decides done.
    let adapter =
        MockAdapter::from_yaml(r#"outcome: { type: completed, summary: "all done, trust me" }"#)
            .unwrap();

    let report = run_task(&t, &adapter, dir.path(), 0).await.unwrap();

    assert_ne!(report.outcome, TaskOutcome::Done);
    assert!(matches!(report.outcome, TaskOutcome::Blocked { .. }));
    assert!(!report.attempts[0].succeeded);
}

#[tokio::test]
async fn a_trivial_criterion_blocks_before_any_attempt_runs() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    // `true` always exits 0 — a non-guard criterion that already passes.
    let t = task("trivial", &["output.txt"], vec![cmd("true")]);
    let adapter = MockAdapter::from_yaml(r#"outcome: { type: completed, summary: "ok" }"#).unwrap();

    let report = run_task(&t, &adapter, dir.path(), 2).await.unwrap();

    assert!(
        report.attempts.is_empty(),
        "no attempt should have been dispatched"
    );
    match report.outcome {
        TaskOutcome::Blocked { reason } => assert!(reason.contains("already passes")),
        other => panic!("expected Blocked, got {other:?}"),
    }
}

#[tokio::test]
async fn a_broken_guard_blocks_before_any_attempt_runs() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    // `false` always exits 1 — a guard that's already red.
    let t = task(
        "broken-guard",
        &["output.txt"],
        vec![cmd("test -f output.txt"), guard("false")],
    );
    let adapter = MockAdapter::from_yaml(r#"outcome: { type: completed, summary: "ok" }"#).unwrap();

    let report = run_task(&t, &adapter, dir.path(), 2).await.unwrap();

    assert!(report.attempts.is_empty());
    match report.outcome {
        TaskOutcome::Blocked { reason } => assert!(reason.contains("guard")),
        other => panic!("expected Blocked, got {other:?}"),
    }
}

#[tokio::test]
async fn an_edit_outside_scope_is_a_violation_even_if_criteria_pass() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    // The criterion only cares about marker.txt (in scope) — but the
    // fixture also writes elsewhere.txt (outside scope). Criteria go
    // green; the task must still not succeed.
    let t = task(
        "narrow-scope",
        &["marker.txt"],
        vec![cmd("test -f marker.txt")],
    );
    let adapter = MockAdapter::from_yaml(
        r#"
effects:
  - { path: marker.txt, content: "in scope\n" }
  - { path: elsewhere.txt, content: "not allowed here\n" }
outcome: { type: completed, summary: "done" }
"#,
    )
    .unwrap();

    let report = run_task(&t, &adapter, dir.path(), 0).await.unwrap();

    assert!(matches!(report.outcome, TaskOutcome::Blocked { .. }));
    assert!(report.attempts[0]
        .post_check
        .iter()
        .all(|c| c.exit_code == 0));
    assert!(!report.attempts[0].scope.violations.is_empty());
}

#[tokio::test]
async fn retries_run_exactly_max_retries_plus_one_attempts_before_blocking() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let t = task(
        "always-red",
        &["output.txt"],
        vec![cmd("test -f output.txt")],
    );
    let adapter = MockAdapter::from_yaml(r#"outcome: { type: completed, summary: "ok" }"#).unwrap();

    let report = run_task(&t, &adapter, dir.path(), 2).await.unwrap();

    assert_eq!(report.attempts.len(), 3); // 1 initial + 2 retries
    assert!(matches!(report.outcome, TaskOutcome::Blocked { .. }));
}

#[tokio::test]
async fn a_crashed_session_is_recorded_and_still_fails_post_check() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let t = task("crash", &["output.txt"], vec![cmd("test -f output.txt")]);
    let adapter = MockAdapter::from_yaml("outcome: { type: crash }").unwrap();

    let report = run_task(&t, &adapter, dir.path(), 0).await.unwrap();

    assert_eq!(report.attempts[0].dispatch, DispatchOutcome::Crashed);
    assert!(!report.attempts[0].succeeded);
}

#[tokio::test]
async fn pre_check_and_post_check_run_every_criterion() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let t = task(
        "two-criteria",
        &["a.txt", "b.txt"],
        vec![cmd("test -f a.txt"), cmd("test -f b.txt")],
    );
    let (runs, outcome) = yunta_engine::pre_check(&t, dir.path()).await.unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(outcome, PreCheckOutcome::Red);
}
