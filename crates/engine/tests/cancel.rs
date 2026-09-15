//! External cancellation of a run. A `CancellationToken` fired while a node
//! holds a session that never ends on its own interrupts that session,
//! pauses the run as `cancelled by user`, and leaves a log that resumes
//! cleanly — the interrupted node runs again and the run finishes. This is
//! the in-engine half of what `yunta cancel` drives from outside the
//! process; the `hang` mock stands in for a stuck agent.

use tokio_util::sync::CancellationToken;
use yunta_core::events::EventPayload;
use yunta_core::events::NodeEvent;
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
