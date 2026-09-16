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
use yunta_testkit::{wait_until_async, Bench};

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

/// The lineage's measurement is a step the run owns, so `yunta cancel`
/// reaches it: the suite dies with the rest of the tree, the log holds
/// no measurement, and the next wake takes it from the top.
#[tokio::test]
async fn a_suite_the_cancellation_stops_leaves_no_measurement_and_the_run_pauses() {
    let token = CancellationToken::new();
    let bench = Bench::new().with_cancel(token.clone());
    // A suite that ends only when something kills it — the shape of a
    // measurement a person interrupts.
    let config = "\
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
baseline:
  suite: \"sleep 3600\"
";
    let workflow = "\
name: measure-me
nodes:
  - id: work
    kind: bash
    run: \"true\"
";

    let (report, ()) = tokio::join!(
        bench.run_with_config(workflow, "sessions: []", config),
        cancel_once_the_suite_is_registered(&bench, &token)
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
            event.payload(),
            Some(EventPayload::Node(NodeEvent::Started(_)))
        )),
        "and no node ran before the measurement the run still owes"
    );
}

/// Fires `token` once the run's registry lists the suite's process
/// group: the suite is the only thing this run has spawned, so the
/// registry naming a group is the suite being under way.
async fn cancel_once_the_suite_is_registered(bench: &Bench, token: &CancellationToken) {
    let run_dir = bench.run_dir();
    wait_until_async(
        || {
            let run_dir = run_dir.clone();
            async move {
                matches!(
                    yunta_engine::read_registry(&run_dir),
                    yunta_engine::Registry::Read(registry)
                        if !registry.doc.process_groups.is_empty()
                )
            }
        },
        || "the suite never registered a process group".to_string(),
    )
    .await;
    token.cancel();
}

/// A birth runs git: a tasks document another run handed over is held to
/// the tree the receiving run opens on, and the answer is a `git
/// merge-base`. Under a token that already fired, that git dies with its
/// tree and the birth says so — a run with no log of its own answers its
/// caller, and nothing is left on disk to resume.
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
