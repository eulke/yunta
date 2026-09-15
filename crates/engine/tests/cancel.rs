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
