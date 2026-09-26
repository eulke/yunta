//! One task from red to done: the pre-check that has to fail, the
//! attempts, the post-check that has to pass, and everything that ends
//! the cycle short of done.
//!
//! An agent's claim is never the verdict — only a criterion passing is —
//! and a criterion that was already red, a guard that was already
//! broken, an edit outside scope, a wall clock or a token budget each
//! close the cycle on their own terms. A criterion is reused only while
//! the tree and the config behind it are unchanged.

use yunta_adapters::MockAdapter;
use yunta_core::events::{
    CriteriaCheckedPayload, CriterionResult, EventPayload, NodeEvent, Phase, TaskLedger,
};
use yunta_core::port::{Budget, PermissionProfile};
use yunta_core::Criterion;
use yunta_core::Task;
use yunta_engine::scope_expansion::GrantLedger;
use yunta_engine::{
    run_task, surprises, AttemptEnv, BlockedCause, CriterionRun, DispatchOutcome, Memo,
    ScopeGovernance, Surprise, TaskOutcome, Unit, UnitId,
};
use yunta_testkit::{init_repo, Owner};
use yunta_testkit_core::Log;

/// The setup a task session of node `build` runs under: the mock
/// runner every fixture here answers as, and nothing else.
/// The setup those task sessions run under, rooted at a run directory
/// of its own: what a capture writes goes under the run, never inside
/// the checkout it measures.
fn bare_setup(run_dir: &std::path::Path) -> yunta_engine::SessionSetup {
    yunta_engine::SessionSetup::bare(
        run_dir.to_path_buf(),
        yunta_core::NodeId::from_static("build"),
        yunta_core::RunnerCandidate {
            adapter: "mock".into(),
            model: "mock-model".into(),
            agent: None,
        },
    )
}

/// A unit to work in — an initialised checkout and the tree it starts
/// from — and the run directory beside it, which is where a session's
/// working files go: the private index a scope audit captures through
/// among them, and it must not sit in the tree it measures.
async fn a_unit(owner: &Owner) -> (tempfile::TempDir, tempfile::TempDir, Unit) {
    let dir = tempfile::tempdir().expect("a checkout");
    let run = tempfile::tempdir().expect("a run directory");
    init_repo(dir.path());
    let from = yunta_engine::head_tree(dir.path(), owner.supervision())
        .await
        .expect("the checkout says where it stands");
    let base = yunta_engine::head_commit(dir.path(), owner.supervision())
        .await
        .expect("and which commit that is");
    let unit = Unit {
        who: UnitId::Task("test-unit".into()),
        worktree: dir.path().to_path_buf(),
        base,
        from,
    };
    (dir, run, unit)
}

/// The node those task sessions belong to: a `loop` node named `build`,
/// declaring nothing of its own.
fn build_node() -> yunta_core::Node {
    serde_norway::from_str("{ id: build, kind: bash, run: \"true\" }")
        .expect("the node the setup names")
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

/// A log that priced nothing: every criterion sorts as a command with
/// no history, so a pre-check meets them in declared order.
fn unpriced() -> TaskLedger {
    TaskLedger::default()
}

/// The tasks fold a run derives from a log whose one pre-check timed
/// each command at the durations `entries` names.
fn priced(entries: &[(&str, &[u64])]) -> TaskLedger {
    let results = entries
        .iter()
        .flat_map(|(cmd, durations)| {
            durations.iter().map(|&duration_ms| CriterionResult {
                cmd: (*cmd).to_string(),
                exit_code: 1,
                r#type: None,
                reused: false,
                duration_ms: Some(duration_ms),
            })
        })
        .collect();
    let events = Log::for_run("run-priced")
        .node(
            "build",
            EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
                task_id: "T1".into(),
                phase: Phase::Pre,
                results,
            })),
        )
        .build();
    yunta_engine::derive(&events).tasks
}

/// The governance a cycle test runs under when governance is not what
/// it is about: no permissions model, the edit rung of the ladder, no
/// scope expansion, and a ledger that has granted nothing.
fn ungoverned(grants: &GrantLedger) -> ScopeGovernance<'_> {
    ScopeGovernance {
        permissions: None,
        profile: PermissionProfile::Edit,
        scope_expansion: None,
        max_expansion_files: 5,
        grants,
        already_granted_paths: &[],
    }
}

fn task(id: &str, scope: &[&str], criteria: Vec<Criterion>) -> Task {
    Task {
        id: id.into(),
        title: "test task".to_string(),
        scope: scope.iter().map(|s| (*s).into()).collect(),
        criteria,
        depends_on: vec![],
        notes: None,
    }
}

#[tokio::test]
async fn a_session_that_makes_the_criterion_pass_reaches_done() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

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

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            max_retries: 2,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
        },
        ungoverned(&GrantLedger::new(0)),
        None,
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap();

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
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

    let t = task(
        "write-output",
        &["output.txt"],
        vec![cmd("test -f output.txt")],
    );
    // The mock reports Completed but its fixture never writes output.txt —
    // this proves the engine, not the
    // agent's self-report, decides done.
    let adapter =
        MockAdapter::from_yaml(r#"outcome: { type: completed, summary: "all done, trust me" }"#)
            .unwrap();

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            max_retries: 0,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
        },
        ungoverned(&GrantLedger::new(0)),
        None,
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap();

    assert_ne!(report.outcome, TaskOutcome::Done);
    assert!(matches!(report.outcome, TaskOutcome::Blocked { .. }));
    assert!(!report.attempts[0].succeeded);
}

#[tokio::test]
async fn a_trivial_criterion_blocks_before_any_attempt_runs() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

    // `true` always exits 0 — a non-guard criterion that already passes.
    let t = task("trivial", &["output.txt"], vec![cmd("true")]);
    let adapter = MockAdapter::from_yaml(r#"outcome: { type: completed, summary: "ok" }"#).unwrap();

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            max_retries: 2,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
        },
        ungoverned(&GrantLedger::new(0)),
        None,
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap();

    assert!(
        report.attempts.is_empty(),
        "no attempt should have been dispatched"
    );
    match report.outcome {
        TaskOutcome::Blocked { cause } => assert_eq!(
            cause.to_string(),
            "criterion `true` already passes before any work — the criteria need fixing, not the task"
        ),
        other => panic!("expected Blocked, got {other:?}"),
    }
}

#[tokio::test]
async fn a_broken_guard_blocks_before_any_attempt_runs() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

    // `false` always exits 1 — a guard that's already red.
    let t = task(
        "broken-guard",
        &["output.txt"],
        vec![cmd("test -f output.txt"), guard("false")],
    );
    let adapter = MockAdapter::from_yaml(r#"outcome: { type: completed, summary: "ok" }"#).unwrap();

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            max_retries: 2,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
        },
        ungoverned(&GrantLedger::new(0)),
        None,
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap();

    assert!(report.attempts.is_empty());
    match report.outcome {
        TaskOutcome::Blocked { cause } => {
            assert_eq!(
                cause.to_string(),
                "guard `false` is already red before any work started"
            )
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
}

#[tokio::test]
async fn an_edit_outside_scope_is_a_violation_even_if_criteria_pass() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

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

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            max_retries: 0,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
        },
        ungoverned(&GrantLedger::new(0)),
        None,
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap();

    assert!(matches!(report.outcome, TaskOutcome::Blocked { .. }));
    assert!(report.attempts[0]
        .post_check
        .iter()
        .all(|c| c.exit_code == 0));
    assert!(!report.attempts[0].scope.violations.is_empty());
}

#[tokio::test]
async fn retries_run_exactly_max_retries_plus_one_attempts_before_blocking() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

    let t = task(
        "always-red",
        &["output.txt"],
        vec![cmd("test -f output.txt")],
    );
    // Every retry is a fresh session, so the fixture scripts one
    // session per expected attempt.
    let adapter = MockAdapter::from_yaml(
        r#"
sessions:
  - outcome: { type: completed, summary: "attempt 1" }
  - outcome: { type: completed, summary: "attempt 2" }
  - outcome: { type: completed, summary: "attempt 3" }
"#,
    )
    .unwrap();

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            max_retries: 2,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
        },
        ungoverned(&GrantLedger::new(0)),
        None,
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap();

    assert_eq!(report.attempts.len(), 3); // 1 initial + 2 retries
    assert!(matches!(report.outcome, TaskOutcome::Blocked { .. }));
}

#[tokio::test]
async fn a_non_retryable_failure_ends_the_cycle() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

    // Criteria stay red (nothing writes the file) and the session reports a
    // failure it marks non-retryable — retrying cannot help, so the cycle
    // stops after the one attempt instead of spending `max_retries` more.
    let t = task("gives-up", &["output.txt"], vec![cmd("test -f output.txt")]);
    // Scripts one session per attempt `max_retries` would allow, so the
    // pre-fix cycle fails on the attempt count, not on running the fixture
    // dry; the fix leaves the extra sessions unconsumed.
    let adapter = MockAdapter::from_yaml(
        r#"
sessions:
  - outcome: { type: failed, message: "unrecoverable", retryable: false }
  - outcome: { type: failed, message: "unrecoverable", retryable: false }
  - outcome: { type: failed, message: "unrecoverable", retryable: false }
"#,
    )
    .unwrap();

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            max_retries: 2,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
        },
        ungoverned(&GrantLedger::new(0)),
        None,
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap();

    assert_eq!(
        report.attempts.len(),
        1,
        "no retry after a non-retryable failure"
    );
    assert!(matches!(report.outcome, TaskOutcome::Blocked { .. }));
    assert!(matches!(
        report.attempts[0].dispatch,
        DispatchOutcome::Failed {
            retryable: false,
            ..
        }
    ));
}

#[tokio::test]
async fn a_crashed_session_is_recorded_and_still_fails_post_check() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

    let t = task("crash", &["output.txt"], vec![cmd("test -f output.txt")]);
    let adapter = MockAdapter::from_yaml("outcome: { type: crash }").unwrap();

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            max_retries: 0,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
        },
        ungoverned(&GrantLedger::new(0)),
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
    let adapter = MockAdapter::from_yaml("outcome: { type: crash }").unwrap();

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            max_retries: 2,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
        },
        ungoverned(&GrantLedger::new(0)),
        None,
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap();

    let TaskOutcome::Blocked {
        cause: BlockedCause::SessionDied(died),
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

#[tokio::test]
async fn pre_check_and_post_check_run_every_criterion() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let t = task(
        "two-criteria",
        &["a.txt", "b.txt"],
        vec![cmd("test -f a.txt"), cmd("test -f b.txt")],
    );
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));
    let runs = yunta_engine::pre_check(&t, dir.path(), &memo, &unpriced(), owner.supervision())
        .await
        .unwrap();
    assert_eq!(runs.len(), 2);
    assert!(surprises(&t, &runs).is_empty());
}

#[tokio::test]
async fn a_criterion_is_reused_when_the_tree_and_config_havent_changed_since_the_last_check() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    // The execution marker lives outside the repo — a criterion is
    // deterministic/read-only by definition, so this only exists
    // to observe whether the command actually ran without itself
    // dirtying the tree tree_hash is computed over (that would
    // self-invalidate the very cache entry it just wrote).
    let marker = root.path().join("executions.txt");

    let t = task(
        "memo",
        &["output.txt"],
        vec![guard(&format!("echo ran >> {} && true", marker.display()))],
    );
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

    let first = yunta_engine::pre_check(&t, &repo, &memo, &unpriced(), owner.supervision())
        .await
        .unwrap();
    assert!(!first[0].reused, "the first check must actually execute");

    let second = yunta_engine::pre_check(&t, &repo, &memo, &unpriced(), owner.supervision())
        .await
        .unwrap();
    assert!(
        second[0].reused,
        "an unchanged tree and config must reuse the cached result"
    );

    let executions = std::fs::read_to_string(&marker).unwrap();
    assert_eq!(
        executions.lines().count(),
        1,
        "the command must have actually run exactly once"
    );
}

#[tokio::test]
async fn a_criterion_re_executes_once_the_tree_changes() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let marker = root.path().join("executions.txt");

    let t = task(
        "memo-invalidation",
        &["output.txt"],
        vec![guard(&format!("echo ran >> {} && true", marker.display()))],
    );
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

    yunta_engine::pre_check(&t, &repo, &memo, &unpriced(), owner.supervision())
        .await
        .unwrap();
    // Dirty the repo's own tree — the next check must see a different
    // tree_hash (the marker file lives outside it and doesn't count).
    std::fs::write(repo.join("new-file.txt"), "changed").unwrap();

    let second = yunta_engine::pre_check(&t, &repo, &memo, &unpriced(), owner.supervision())
        .await
        .unwrap();
    assert!(
        !second[0].reused,
        "a changed tree must invalidate the memoized result"
    );

    let executions = std::fs::read_to_string(&marker).unwrap();
    assert_eq!(executions.lines().count(), 2);
}

#[tokio::test]
async fn a_hung_session_is_cut_by_the_wall_clock_timeout() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

    let t = task("timeout", &["output.txt"], vec![cmd("test -f output.txt")]);
    let adapter = MockAdapter::from_yaml("outcome: { type: hang }").unwrap();
    let budget = Budget {
        timeout: Some(std::time::Duration::from_millis(50)),
        ..Default::default()
    };

    // The test itself times out (failing loudly) if run_task doesn't
    // return promptly — a stuck session must
    // never block the engine forever.
    let report = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        run_task(
            &t,
            "Implement your task.",
            AttemptEnv {
                node: &build_node(),
                adapter: &adapter,
                unit: &unit,
                max_retries: 0,
                budget,
                memo: &memo,
                history: &unpriced(),
                supervision: owner.supervision(),
            },
            ungoverned(&GrantLedger::new(0)),
            None,
            &tokio_util::sync::CancellationToken::new(),
            &bare_setup(run.path()),
        ),
    )
    .await
    .expect("run_task must return once its own budget timeout elapses")
    .unwrap();

    match &report.attempts[0].dispatch {
        DispatchOutcome::BudgetExceeded { reason } => {
            assert_eq!(reason, "exceeded timeout of 50ms")
        }
        other => panic!("expected BudgetExceeded, got {other:?}"),
    }
}

#[tokio::test]
async fn exceeding_max_tokens_cuts_the_session_before_its_outcome() {
    let owner = Owner::new();
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

    let t = task(
        "token-limit",
        &["marker.txt"],
        vec![cmd("test -f marker.txt")],
    );
    // Two usage steps totalling 150 tokens, then a completion that
    // dispatch must never see because the budget is 100.
    let adapter = MockAdapter::from_yaml(
        r#"
steps:
  - { type: usage, input_tokens: 60, output_tokens: 20 }
  - { type: usage, input_tokens: 50, output_tokens: 20 }
outcome: { type: completed, summary: "should never be reached" }
"#,
    )
    .unwrap();
    let budget = Budget {
        max_tokens: Some(100),
        ..Default::default()
    };

    let report = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            max_retries: 0,
            budget,
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
        },
        ungoverned(&GrantLedger::new(0)),
        None,
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap();

    match &report.attempts[0].dispatch {
        DispatchOutcome::BudgetExceeded { reason } => {
            assert_eq!(reason, "exceeded max_tokens 100 (150 used)")
        }
        other => panic!("expected BudgetExceeded, got {other:?}"),
    }
}

// --- criterion ordering by the duration the log recorded ---------------

#[tokio::test]
async fn pre_check_orders_criteria_by_the_median_duration_the_log_recorded() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));
    let costly = "test -f never.txt";
    let cheap = "test -f also-never.txt";
    let t = task("T1", &["**"], vec![cmd(costly), cmd(cheap)]);

    // A log that priced nothing: declared order, and this pass records
    // what each command cost.
    let runs = yunta_engine::pre_check(&t, dir.path(), &memo, &unpriced(), owner.supervision())
        .await
        .unwrap();
    assert!(surprises(&t, &runs).is_empty());
    assert_eq!(runs[0].cmd, costly);
    assert_eq!(runs[1].cmd, cheap);
    assert!(
        runs.iter().all(|run| run.duration_ms.is_some()),
        "executed criteria must record their duration: {runs:?}"
    );

    // The tree changes (no memo reuse), and the medians the log holds
    // reorder the pass: the cheap command runs first to fail fast.
    std::fs::write(dir.path().join("changed.txt"), "x").unwrap();
    let history = priced(&[(costly, &[400, 600]), (cheap, &[5, 7])]);
    let runs = yunta_engine::pre_check(&t, dir.path(), &memo, &history, owner.supervision())
        .await
        .unwrap();
    assert!(
        surprises(&t, &runs).is_empty(),
        "ordering never alters the verdict"
    );
    assert_eq!(
        runs[0].cmd, cheap,
        "the order the log priced must put the cheap criterion first"
    );
    assert_eq!(runs[1].cmd, costly);
}

#[tokio::test]
async fn pre_check_uses_medians_with_outliers_and_stable_declaration_ties() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));
    let slow = "test -f slow.txt";
    let tied_first = "test -f tied-first.txt";
    let tied_second = "test -f tied-second.txt";
    let unpriced_command = "test -f unpriced.txt";
    let t = task(
        "T1",
        &["**"],
        vec![
            cmd(slow),
            cmd(tied_first),
            cmd(unpriced_command),
            cmd(tied_second),
        ],
    );
    let history = priced(&[
        (slow, &[1, 50, 99]),
        (tied_first, &[12, 12, 1000]),
        (tied_second, &[1000, 12, 12]),
    ]);

    let runs = yunta_engine::pre_check(&t, dir.path(), &memo, &history, owner.supervision())
        .await
        .unwrap();

    assert_eq!(
        runs.iter().map(|run| run.cmd.as_str()).collect::<Vec<_>>(),
        [tied_first, tied_second, slow, unpriced_command],
        "outliers do not change medians, equal medians retain declaration order, and unknown history sorts last"
    );
    assert_eq!(runs.len(), t.criteria.len(), "every criterion still runs");
    assert!(
        surprises(&t, &runs).is_empty(),
        "ordering does not alter the verdict"
    );
}

#[tokio::test]
async fn reused_criteria_carry_no_duration() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));
    let t = task("T1", &["**"], vec![cmd("test -f never.txt")]);

    let runs = yunta_engine::pre_check(&t, dir.path(), &memo, &unpriced(), owner.supervision())
        .await
        .unwrap();
    assert!(!runs[0].reused);
    assert!(runs[0].duration_ms.is_some());

    // Same tree: the memo answers, and a reused result has no duration
    // of its own (nothing ran).
    let runs = yunta_engine::pre_check(&t, dir.path(), &memo, &unpriced(), owner.supervision())
        .await
        .unwrap();
    assert!(runs[0].reused);
    assert!(runs[0].duration_ms.is_none());
}

#[tokio::test]
async fn criterion_declaration_order_never_alters_the_pre_check_verdict() {
    let owner = Owner::new();
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    // A trivially-green criterion among red ones: the verdict must be
    // TrivialCriterion no matter how the declaration is permuted.
    let criteria = vec![cmd("test -f never.txt"), cmd("true"), guard("true")];
    let mut permutations: Vec<Vec<Criterion>> = vec![
        criteria.clone(),
        criteria.iter().rev().cloned().collect(),
        vec![
            criteria[1].clone(),
            criteria[2].clone(),
            criteria[0].clone(),
        ],
    ];
    let mut verdicts = Vec::new();
    for (i, permutation) in permutations.drain(..).enumerate() {
        let memo = Memo::new(yunta_core::sha256_hex(format!("config-{i}").as_bytes()));
        let t = task("T1", &["**"], permutation);
        let runs = yunta_engine::pre_check(&t, dir.path(), &memo, &unpriced(), owner.supervision())
            .await
            .unwrap();
        verdicts.push(surprises(&t, &runs));
    }
    assert!(
        verdicts
            .iter()
            .all(|found| matches!(found.as_slice(), [Surprise::TrivialCriterion { .. }])),
        "got: {verdicts:?}"
    );
}

#[test]
fn criterion_results_without_duration_still_parse() {
    // Additive payload evolution — an older event without
    // `duration_ms` parses, and the field reads back `None`.
    let old = r#"{ "cmd": "cargo test", "exit_code": 0, "reused": false }"#;
    let result: yunta_core::events::CriterionResult = serde_json::from_str(old).unwrap();
    assert_eq!(result.duration_ms, None);
}

/// A `SessionObserver` whose every append fails, standing in for storage
/// that has gone down mid-session.
struct FailingObserver;

#[async_trait::async_trait]
impl yunta_engine::SessionObserver for FailingObserver {
    async fn record(
        &self,
        _node_id: &yunta_core::NodeId,
        _payload: yunta_core::events::EventPayload,
    ) -> Result<yunta_core::Seq, yunta_storage::StorageError> {
        Err(yunta_storage::StorageError::Append {
            run_id: yunta_core::RunId::from("run-test"),
            source: "audit storage is down".into(),
        })
    }

    fn process_registry(&self) -> Option<&yunta_engine::ProcessRegistry> {
        None
    }
}

#[tokio::test]
async fn a_lost_session_audit_event_fails_the_task() {
    let owner = Owner::new();
    // A session's audit event that cannot be appended is not dropped
    // with a warning: the storage cause travels back and fails the task,
    // so the trail never silently loses an event.
    let (_dir, run, unit) = a_unit(&owner).await;
    let memo = Memo::new(yunta_core::sha256_hex(b"config-hash"));

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

    let node = yunta_core::NodeId::from("build");
    let observer = FailingObserver;
    let err = run_task(
        &t,
        "Implement your task.",
        AttemptEnv {
            node: &build_node(),
            adapter: &adapter,
            unit: &unit,
            max_retries: 2,
            budget: Budget::default(),
            memo: &memo,
            history: &unpriced(),
            supervision: owner.supervision(),
        },
        ungoverned(&GrantLedger::new(0)),
        Some((&observer as &dyn yunta_engine::SessionObserver, &node)),
        &tokio_util::sync::CancellationToken::new(),
        &bare_setup(run.path()),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, yunta_engine::TaskCycleError::Audit { .. }),
        "a lost session audit event must fail the task, got: {err:?}"
    );
}

/// The verdict is a function of what ran, not of the order it ran in
/// (D177): the pre-check evaluates the whole set and names every
/// criterion that already passes and every guard already red, in the
/// order the task declares them.
#[test]
fn surprises_names_every_trivial_criterion_and_every_broken_guard_in_declaration_order() {
    let t = task(
        "a-whole-set",
        &["out.txt"],
        vec![
            cmd("test -f out.txt"),
            guard("lint"),
            cmd("already-green"),
            guard("build"),
        ],
    );
    // Run out of declared order, the way the learned ordering runs them.
    let runs = vec![
        ran("already-green", 0, false),
        ran("build", 1, true),
        ran("test -f out.txt", 1, false),
        ran("lint", 0, true),
    ];

    assert_eq!(
        surprises(&t, &runs),
        vec![
            Surprise::TrivialCriterion {
                cmd: "already-green".to_string()
            },
            Surprise::BrokenGuard {
                cmd: "build".to_string()
            },
        ],
        "every surprise, in the order the task declares its criteria"
    );

    let all_red = vec![
        ran("test -f out.txt", 1, false),
        ran("lint", 0, true),
        ran("already-green", 1, false),
        ran("build", 0, true),
    ];
    assert!(
        surprises(&t, &all_red).is_empty(),
        "nothing prejudged is the normal case"
    );
}

/// One `criterion_checked`, as the pre-check records it.
fn ran(cmd: &str, exit_code: i32, is_guard: bool) -> CriterionRun {
    CriterionRun {
        cmd: cmd.to_string(),
        exit_code,
        is_guard,
        reused: false,
        duration_ms: None,
    }
}
