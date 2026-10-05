//! A session cut off from the service behind it waits for the service to
//! answer and goes on as the same session — never past a cancellation, and
//! never past a bound of the host's awake time, after which it fails the
//! way any session that failed does.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{EventPayload, NodeEvent, SessionEvent, StoredEvent};
use yunta_engine::{Observed, RunObserver, RunReport, RunTerminal};
use yunta_testkit::Bench;
use yunta_testkit_core::RushingClock;

const WORKFLOW: &str =
    "name: offline\nnodes:\n  - id: work\n    kind: prompt\n    runner: executor\n    prompt: \"Do the work.\"\n";

/// What Claude Code prints when its machine cannot resolve the API.
const LOST: &str = "API Error: Can't reach the API server — check your internet or DNS (ENOTFOUND)";

/// One session that loses its service, and what it does once picked back
/// up; the service answers after `down_for` probes.
fn fixture(down_for: u32) -> String {
    format!(
        "capabilities: {{ resume_session: true }}\nservice_down_for: {down_for}\nsessions:\n\
         \x20 - outcome: {{ type: failed, message: {LOST:?}, retryable: true }}\n\
         \x20 - match_prompt_contains: \"dropped and is back\"\n    outcome: {{ type: completed, summary: done }}\n"
    )
}

fn said(events: &[StoredEvent], kind: &str) -> bool {
    events.iter().any(|event| event.body.kind_name() == kind)
}

#[tokio::test]
async fn unreachable_session_waits_and_resumes_same_id() {
    let bench = Bench::new();

    let RunReport { terminal, .. } = bench.run(WORKFLOW, &fixture(1)).await;

    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    assert!(said(&events, "service_unreachable") && said(&events, "service_reachable"));
    let lost = events.iter().find_map(|event| match event.payload() {
        Some(EventPayload::Session(SessionEvent::ServiceUnreachable(p))) => {
            Some(p.session_id.clone())
        }
        _ => None,
    });
    assert_eq!(
        bench.mock().resumes_seen(),
        lost.into_iter().collect::<Vec<_>>()
    );
}

/// Cancels the run the moment its session loses its service.
struct CancelOnCutOff(CancellationToken);

impl RunObserver for CancelOnCutOff {
    fn observe(&self, event: Observed<'_>) {
        if let EventPayload::Session(SessionEvent::ServiceUnreachable(_)) = event.payload {
            self.0.cancel();
        }
    }
}

#[tokio::test]
async fn wait_is_cancellable() {
    let cancel = CancellationToken::new();
    let bench = Bench::new()
        .with_cancel(cancel.clone())
        .with_observer(Arc::new(CancelOnCutOff(cancel)));

    let down = fixture(u32::MAX);
    let run = bench.run(WORKFLOW, &down);
    let RunReport { terminal, .. } = tokio::time::timeout(Duration::from_secs(30), run)
        .await
        .expect("a cancelled wait ends the run");

    assert_ne!(terminal, RunTerminal::Finished);
    let events = bench.events();
    assert!(said(&events, "service_unreachable") && !said(&events, "service_reachable"));
}

#[tokio::test]
async fn past_bound_fails_as_today() {
    // Twenty minutes of the host's awake time pass with every reading, so
    // the wait is past its bound by its second look.
    let clock = Arc::new(RushingClock::by(Duration::from_secs(20 * 60)));
    let bench = Bench::new().with_clock(clock);

    let RunReport { terminal, .. } = bench.run(WORKFLOW, &fixture(u32::MAX)).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let events = bench.events();
    assert!(said(&events, "service_unreachable") && !said(&events, "service_reachable"));
    let failed = events.iter().find_map(|event| match event.payload() {
        Some(EventPayload::Node(NodeEvent::Failed(p))) => Some(p.failure.to_string()),
        _ => None,
    });
    assert!(failed.is_some_and(|failure| failure.contains("ENOTFOUND")));
}
