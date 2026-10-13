//! A task's failed attempt, as the cycle reports it: a red attempt blocks the
//! task without another session, a blocked task says what its red criterion
//! printed last, a non-retryable failure ends the cycle, and a crashed or dead
//! session is recorded and its task names what ended it.

use super::*;

#[tokio::test]
async fn a_red_attempt_blocks_the_task_without_opening_another_session() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

    let t = task(
        "always-red",
        &["output.txt"],
        vec![cmd("test -f output.txt")],
    );
    // Three scripted, so a cycle that opened more would find them.
    let adapter = scripted(
        r#"
dialect: claude-code
sessions:
  - outcome: { type: completed, summary: "attempt 1" }
  - outcome: { type: completed, summary: "attempt 2" }
  - outcome: { type: completed, summary: "attempt 3" }
"#,
    );

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
            carry: None,
            resume: None,
        },
        ungoverned(&GrantWindow::seeded(0)),
        None,
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap();

    assert!(matches!(
        report.outcome,
        TaskOutcome::Blocked {
            cause: TaskCause::Unmet { attempts: 1, .. }
        }
    ));
}

#[tokio::test]
async fn a_blocked_task_says_what_its_red_criterion_printed_last() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let t = task(
        "still-red",
        &["output.txt"],
        vec![cmd("echo 'output.txt is missing' >&2; test -f output.txt")],
    );
    let adapter = scripted(
        r#"
dialect: claude-code
sessions:
  - outcome: { type: completed, summary: "attempt 1" }
"#,
    );
    let report = run_once(&owner, &unit, run.path(), &t, &adapter, Budget::default()).await;
    let TaskOutcome::Blocked {
        cause: TaskCause::Unmet {
            attempts: 1, red, ..
        },
    } = report.outcome
    else {
        panic!("the attempt left the criterion red: {:?}", report.outcome);
    };
    assert_eq!(
        red.iter()
            .map(|result| (result.exit_code, result.conclusion.as_deref()))
            .collect::<Vec<_>>(),
        vec![(1, Some("output.txt is missing"))],
        "whoever decides reads why it fails, not only that it does"
    );
}

#[tokio::test]
async fn a_non_retryable_failure_ends_the_cycle() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

    // Criteria stay red (nothing writes the file) and the session refuses
    // with a message no retry cures: the task blocks on what the session
    // said, after the one attempt.
    let t = task("gives-up", &["output.txt"], vec![cmd("test -f output.txt")]);
    // Scripts more sessions than it needs, so a cycle that opened another
    // would find one.
    let adapter = scripted(
        r#"
dialect: claude-code
sessions:
  - outcome: { type: failed, message: "unrecoverable" }
  - outcome: { type: failed, message: "unrecoverable" }
  - outcome: { type: failed, message: "unrecoverable" }
"#,
    );

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
            carry: None,
            resume: None,
        },
        ungoverned(&GrantWindow::seeded(0)),
        None,
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap();

    assert_eq!(report.attempts.len(), 1, "no retry after a refused session");
    let TaskOutcome::Blocked {
        cause: TaskCause::SessionRefused(refusal),
    } = &report.outcome
    else {
        panic!("a refused session blocks the task: {:?}", report.outcome);
    };
    assert_eq!(refusal.adapter, "mock");
    assert_eq!(refusal.message, "unrecoverable");
    assert_eq!(refusal.cause, RefusalCause::Final);
    assert!(matches!(
        report.attempts[0].dispatch,
        DispatchOutcome::Failed { .. }
    ));
}

#[tokio::test]
async fn a_crashed_session_is_recorded_and_still_fails_post_check() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

    let t = task("crash", &["output.txt"], vec![cmd("test -f output.txt")]);
    let adapter = scripted("dialect: claude-code\noutcome: { type: crash }");

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
            carry: None,
            resume: None,
        },
        ungoverned(&GrantWindow::seeded(0)),
        None,
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap();

    // The mock has no process of its own, so there is nothing to ask
    // about how one ended.
    assert_eq!(
        report.attempts[0].dispatch,
        DispatchOutcome::Crashed { exit: None }
    );
    assert!(!report.attempts[0].succeeded);
}

/// A session that says nothing leaves no work behind and no criteria
/// worth re-running: the task blocks naming the death, so a reader is
/// not left to infer a dead CLI from criteria that never ran.
#[tokio::test]
async fn a_task_whose_session_died_blocks_naming_the_exit() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));
    let t = task("crash", &["output.txt"], vec![cmd("test -f output.txt")]);
    let adapter = scripted("dialect: claude-code\noutcome: { type: crash }");

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
            carry: None,
            resume: None,
        },
        ungoverned(&GrantWindow::seeded(0)),
        None,
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap();

    let TaskOutcome::Blocked {
        cause: TaskCause::SessionDied(died),
    } = &report.outcome
    else {
        panic!("a dead session blocks the task: {:?}", report.outcome);
    };
    assert_eq!(died.adapter, "mock");
    assert_eq!(
        report.attempts.len(),
        1,
        "and stops there: the next attempt would open the same session"
    );
}
