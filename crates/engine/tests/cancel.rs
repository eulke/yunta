//! External cancellation of a run. A `CancellationToken` fired while a node
//! holds a session that never ends on its own interrupts that session,
//! pauses the run as `cancelled by user`, and leaves a log that resumes
//! cleanly — the interrupted node runs again and the run finishes. This is
//! the in-engine half of what `yunta cancel` drives from outside the
//! process; the `hang` mock stands in for a stuck agent.

use tokio_util::sync::CancellationToken;
use yunta_core::events::EventPayload;
use yunta_core::events::{NodeEvent, RunEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{wait_until_async, Bench, MOCK_CONFIG};

const HANGING_WORKFLOW: &str = "\
name: cancel-me
nodes:
  - id: work
    kind: prompt
    runner: executor
    prompt: work
";

/// A single session that opens and then never produces a terminal event on
/// its own — only an interrupt or kill ends it.
const HANG_FIXTURE: &str = "sessions:\n  - { outcome: { type: hang } }\n";

/// A single session that finishes at once — what the interrupted node meets
/// when the run resumes.
const COMPLETING_FIXTURE: &str = "sessions:\n  - { outcome: { type: completed, summary: done } }\n";

/// Fires `token` as soon as the run's node is under way. A hung run cannot
/// finish on its own, so waiting for `node_started` to land on the log —
/// never a timer — makes the cancellation deterministic.
async fn cancel_once_started(bench: &Bench, token: &CancellationToken) {
    wait_until_async(
        || async move {
            bench.events().iter().any(|event| {
                matches!(
                    event.payload(),
                    Some(EventPayload::Node(NodeEvent::Started(_)))
                )
            })
        },
        || "the hanging node never started".to_string(),
    )
    .await;
    token.cancel();
}

#[tokio::test]
async fn cancel_pauses_a_hanging_run_as_cancelled_by_user() {
    let token = CancellationToken::new();
    let bench = Bench::new().with_cancel(token.clone());

    let (report, ()) = tokio::join!(
        bench.run(HANGING_WORKFLOW, HANG_FIXTURE),
        cancel_once_started(&bench, &token)
    );

    let RunReport { terminal, .. } = report;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(reason, "cancelled by user");
        }
        other => panic!("a cancelled run must pause, got {other:?}"),
    }
}

#[tokio::test]
async fn cancel_then_resume_finishes_the_run() {
    let token = CancellationToken::new();
    let bench = Bench::new().with_cancel(token.clone());

    // First: cancel the hanging run, exactly as the test above does.
    let (first, ()) = tokio::join!(
        bench.run(HANGING_WORKFLOW, HANG_FIXTURE),
        cancel_once_started(&bench, &token)
    );
    let RunReport { terminal, .. } = first;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));

    // Then: resume with a session that completes, under a token nobody
    // fires. Resume re-derives the log and re-runs the interrupted node,
    // which finishes the run.
    let bench = bench.with_cancel(CancellationToken::new());
    let RunReport { terminal, .. } = bench.wake_on_fixture(COMPLETING_FIXTURE).await;
    assert_eq!(terminal, RunTerminal::Finished);
}

/// A node with a scope of its own, which is given a checkout before it
/// starts: the checkout is a step's `git worktree add`, outside any
/// node.
const SCOPED_WORKFLOW: &str = "\
name: scoped
nodes:
  - id: work
    kind: bash
    run: \"true\"
    scope: [\"*.txt\"]
";

/// A cancellation that stops the git a step runs outside any node — the
/// checkout a scoped node is given before it starts — pauses the run as
/// cancelled by user, and the next wake finishes it.
#[tokio::test]
async fn a_cancel_that_stops_a_step_s_git_pauses_the_run() {
    let token = CancellationToken::new();
    let stubs = tempfile::tempdir().expect("a directory for the stub");
    let held = stubs.path().join("worktree-add.pid");
    let armed = stubs.path().join("armed");
    tokio::fs::write(&armed, "").await.expect("arm the hold");
    let vars = yunta_testkit::stubs::git_holding(stubs.path(), "worktree add", &held, Some(&armed));
    let bench = Bench::new()
        .with_cancel(token.clone())
        .with_subprocess_vars(vars);

    let (report, ()) = tokio::join!(
        bench.run(SCOPED_WORKFLOW, "sessions: []"),
        cancel_once_written(&held, &token)
    );

    assert_eq!(
        report.terminal,
        RunTerminal::Paused {
            reason: "cancelled by user".to_string()
        }
    );
    assert!(
        matches!(
            bench.events().last().and_then(|event| event.payload()),
            Some(EventPayload::Run(RunEvent::Paused(_)))
        ),
        "the log ends on the pause: {:#?}",
        bench.events()
    );
    tokio::fs::remove_file(&armed)
        .await
        .expect("disarm the hold");
    let bench = bench.with_cancel(CancellationToken::new());
    let RunReport { terminal, .. } = bench.wake_on_fixture("sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
}

/// Fires `token` once `marker` is on disk: whatever writes it is under
/// way when it appears.
async fn cancel_once_written(marker: &std::path::Path, token: &CancellationToken) {
    wait_until_async(
        || async { tokio::fs::try_exists(marker).await.unwrap_or(false) },
        || format!("`{}` was never written", marker.display()),
    )
    .await;
    token.cancel();
}

/// Work, then the comparison that reads the lineage's measurement.
const WORK_THEN_COMPARED: &str = "\
name: measure-me
nodes:
  - id: work
    kind: bash
    run: \"true\"
  - id: regressions
    kind: check
    builtin: baseline_compare
    depends_on: [work]
";

/// The lineage's measurement is a step the run owns, so `yunta cancel`
/// reaches it: the suite dies with the rest of the tree, the log holds
/// no measurement, the node that reads it never starts, and the next
/// wake takes it from the top.
#[tokio::test]
async fn a_suite_the_cancellation_stops_leaves_no_measurement_and_the_run_pauses() {
    let token = CancellationToken::new();
    let bench = Bench::new().with_cancel(token.clone());
    // A suite that ends only when something kills it — the shape of a
    // measurement a person interrupts — and says when it is under way.
    let started = bench.run_dir().with_extension("suite-started");
    let config = format!(
        "{MOCK_CONFIG}baseline:\n  suite: \"touch {}; sleep 3600\"\n",
        started.display()
    );

    let (report, ()) = tokio::join!(
        bench.run_with_config(WORK_THEN_COMPARED, "sessions: []", &config),
        cancel_once_written(&started, &token)
    );

    let RunReport { terminal, .. } = report;
    match terminal {
        RunTerminal::Paused { reason } => assert_eq!(reason, "cancelled by user"),
        other => panic!("a cancelled run must pause, got {other:?}"),
    }
    assert!(
        bench.events().iter().all(|event| !matches!(
            event.payload(),
            Some(EventPayload::Run(RunEvent::BaselineCaptured(_)))
        )),
        "a suite nobody let finish measured nothing: {:#?}",
        bench.events()
    );
    assert!(
        bench.events().iter().all(|event| !matches!(
            (
                event.payload(),
                event.node_id.as_ref().map(|id| id.as_str())
            ),
            (
                Some(EventPayload::Node(NodeEvent::Started(_))),
                Some("regressions")
            )
        )),
        "and the node that reads the measurement never started"
    );
}

#[tokio::test]
async fn a_cancelled_token_stops_the_git_a_birth_runs() {
    let token = CancellationToken::new();
    let staging = Bench::new().with_cancel(token.clone());
    let handed = handed_over_document(&staging);
    let bench = staging.born_holding(vec![handed]);

    // The token fires between the freeze and the birth, so the birth's
    // own git is the first thing it meets.
    let error = bench
        .try_create_after(ONE_NODE, "sessions: []", yunta_testkit::MOCK_CONFIG, || {
            token.cancel();
        })
        .await
        .expect_err("a birth whose git was killed cannot say what the tree carries");

    assert!(
        matches!(error, yunta_engine::RunError::Cancelled),
        "a stopped git is the invocation being cancelled, not the birth failing: {error:?}"
    );
    assert!(
        bench.events().is_empty(),
        "the birth reads what it was handed before it writes anything, so a cancelled one \
         leaves no run"
    );
    assert!(
        !bench.runs_root.join(bench.run_id.as_str()).exists(),
        "and no directory either"
    );
}

const ONE_NODE: &str = "name: born\nnodes:\n  - { id: work, kind: bash, run: \"true\" }\n";

/// A tasks document another run finished a task of, at a commit
/// `bench`'s tree carries — what makes a birth ask git whether the work
/// crossed.
fn handed_over_document(bench: &Bench) -> yunta_engine::BirthArtifact {
    yunta_testkit::write(&bench.worktree.join("a.txt"), "a\n");
    yunta_testkit::git(&bench.worktree, &["add", "."]);
    yunta_testkit::git(&bench.worktree, &["commit", "-q", "-m", "a"]);
    let landed: yunta_core::CommitSha =
        yunta_testkit::git_output(&bench.worktree, &["rev-parse", "HEAD"])
            .parse()
            .expect("a commit sha");

    let document = yunta_testkit::tasks_document(&[("T001", "a.txt", "test -f a.txt")]);
    let source = yunta_core::RunId::from("run-source");
    let log = yunta_testkit::SourceLog::open(
        &bench.storage,
        &source,
        std::sync::Arc::new(yunta_testkit_core::FixedClock),
    );
    let registered = log.record(yunta_testkit::task_registered(&document.tasks[0]));
    log.record(yunta_testkit::status_changed_carrying(
        &document.tasks[0].id,
        yunta_core::events::TaskStatus::Done,
        &landed,
        registered,
    ));

    yunta_engine::BirthArtifact {
        artifact: yunta_core::events::ArtifactId::Interpreted {
            kind: yunta_core::ArtifactKind::Tasks,
        },
        origin: yunta_engine::BirthOrigin::Inherited {
            run: source,
            producer: None,
        },
        bytes: yunta_core::shape::render(&document)
            .expect("the canonical rendering")
            .into_bytes(),
    }
}
