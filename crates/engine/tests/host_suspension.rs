//! A run notices when the host it works on slept, and says so on its log
//! before anything that happened after. No session opens until the host
//! has stayed awake a while since.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{EventPayload, NodeEvent, RunEvent, SessionEvent, StoredEvent};
use yunta_engine::{RunReport, RunTerminal, SETTLE_AFTER_SUSPENSION};
use yunta_testkit::{wait_until_async, Bench, WAIT_DEADLINE};
use yunta_testkit_core::HostClock;

/// One node that reaches a meeting point outside the run's tree, then
/// holds until the test lets it go.
const HOLDS: &str = r#"
name: holds
nodes:
  - id: work
    kind: bash
    run: "mkdir -p '{{run.dir}}/meet' && touch '{{run.dir}}/meet/ready' && while [ ! -f '{{run.dir}}/meet/go' ]; do sleep 0.05; done"
"#;

/// A node that opens a session once the one at the meeting point is done.
const THEN_ASKS: &str =
    "  - { id: ask, kind: prompt, runner: executor, prompt: ask, depends_on: [work] }\n";

const COMPLETES: &str = "sessions:\n  - { outcome: { type: completed, summary: done } }\n";

/// Where the node meets the test.
fn meeting(bench: &Bench) -> PathBuf {
    bench.run_dir().join("meet")
}

/// Every suspension the log records, with its position.
fn suspensions(events: &[StoredEvent]) -> Vec<(u64, u64)> {
    events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Run(RunEvent::HostSuspended(p))) => {
                Some((event.seq.get(), p.slept_ms))
            }
            _ => None,
        })
        .collect()
}

fn finished_at(events: &[StoredEvent], node: &str) -> u64 {
    events
        .iter()
        .find(|event| {
            event.node_id.as_ref().is_some_and(|id| id.as_str() == node)
                && matches!(
                    event.payload(),
                    Some(EventPayload::Node(NodeEvent::Finished(_)))
                )
        })
        .map(|event| event.seq.get())
        .expect("the node finished")
}

#[tokio::test]
async fn a_suspension_during_a_node_is_on_the_log_before_the_node_closes() {
    let host = Arc::new(HostClock::default());
    let bench = Bench::new().with_clock(host.clone());
    let meet = meeting(&bench);

    let (RunReport { terminal, .. }, ()) =
        tokio::join!(bench.run(HOLDS, "sessions: []\n"), async {
            let ready = meet.join("ready");
            wait_until_async(
                || {
                    let ready = ready.clone();
                    async move { ready.exists() }
                },
                || "the node never reached its meeting point".to_string(),
            )
            .await;
            host.suspend(Duration::from_secs(3_600));
            tokio::fs::write(meet.join("go"), "").await.unwrap();
        });

    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    let recorded = suspensions(&events);
    let [(at, slept_ms)] = recorded[..] else {
        panic!("exactly one suspension: {recorded:?}");
    };
    assert_eq!(slept_ms, 3_600_000, "the hour the wall clock moved alone");
    assert!(
        at < finished_at(&events, "work"),
        "noticed before the node closed"
    );
}

/// A clock whose wall time stands still while the process's own time
/// moves reads as a host that never slept.
#[tokio::test]
async fn a_run_whose_wall_clock_is_frozen_records_no_suspension() {
    let bench = Bench::new();
    let workflow = r#"
name: slow
nodes:
  - { id: first, kind: bash, run: "sleep 1.2" }
  - { id: second, kind: bash, run: "true", depends_on: [first] }
"#;
    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []\n").await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(suspensions(&bench.events()), vec![]);
}

/// Lets the node at the meeting point go once it is there, the host having
/// slept an hour in between.
async fn sleep_through_the_meeting(bench: &Bench, host: &HostClock) {
    let ready = meeting(bench).join("ready");
    wait_until_async(
        || {
            let ready = ready.clone();
            async move { ready.exists() }
        },
        || "the node never reached its meeting point".to_string(),
    )
    .await;
    host.suspend(Duration::from_secs(3_600));
    tokio::fs::write(meeting(bench).join("go"), "")
        .await
        .unwrap();
}

/// Waits until the log holds what `holds` looks for.
async fn until(bench: &Bench, holds: fn(&[StoredEvent]) -> bool, what: &str) {
    wait_until_async(
        || async move { holds(&bench.events()) },
        || format!("the log never showed {what}"),
    )
    .await;
}

fn asking(events: &[StoredEvent]) -> bool {
    events.iter().any(|event| {
        event
            .node_id
            .as_ref()
            .is_some_and(|id| id.as_str() == "ask")
            && matches!(
                event.payload(),
                Some(EventPayload::Node(NodeEvent::Started(_)))
            )
    })
}

fn a_session_opened(events: &[StoredEvent]) -> bool {
    events.iter().any(|event| {
        matches!(
            event.payload(),
            Some(EventPayload::Session(SessionEvent::Opened(_)))
        )
    })
}

#[tokio::test]
async fn no_session_opens_until_the_host_settles() {
    let host = Arc::new(HostClock::default());
    let bench = Bench::new().with_clock(host.clone());
    let workflow = format!("{HOLDS}{THEN_ASKS}");

    let (RunReport { terminal, .. }, ()) = tokio::join!(bench.run(&workflow, COMPLETES), async {
        sleep_through_the_meeting(&bench, &host).await;
        until(&bench, asking, "the prompt node starting").await;
        let early = tokio::time::timeout(
            Duration::from_secs(2),
            until(&bench, a_session_opened, "a session opening"),
        )
        .await;
        assert!(early.is_err(), "a session opened on a host that just woke");
        host.advance(SETTLE_AFTER_SUSPENSION);
    });

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(a_session_opened(&bench.events()));
}

/// Only sessions wait: a command runs on a host that just woke.
#[tokio::test]
async fn a_command_runs_while_sessions_wait_for_the_host_to_settle() {
    let host = Arc::new(HostClock::default());
    let bench = Bench::new().with_clock(host.clone());
    let workflow =
        format!("{HOLDS}  - {{ id: after, kind: bash, run: \"true\", depends_on: [work] }}\n");

    let (RunReport { terminal, .. }, ()) = tokio::join!(
        bench.run(&workflow, "sessions: []\n"),
        sleep_through_the_meeting(&bench, &host)
    );

    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn a_cancelled_run_stops_waiting_for_the_host() {
    let host = Arc::new(HostClock::default());
    let token = CancellationToken::new();
    let bench = Bench::new()
        .with_clock(host.clone())
        .with_cancel(token.clone());
    let workflow = format!("{HOLDS}{THEN_ASKS}");

    let (report, ()) = tokio::join!(
        tokio::time::timeout(WAIT_DEADLINE, bench.run(&workflow, COMPLETES)),
        async {
            sleep_through_the_meeting(&bench, &host).await;
            until(&bench, asking, "the prompt node starting").await;
            token.cancel();
        }
    );
    let RunReport { terminal, .. } = report.expect("the cancellation ends the wait");

    assert!(
        matches!(&terminal, RunTerminal::Paused { reason } if reason == "cancelled by user"),
        "{terminal:?}"
    );
    assert!(!a_session_opened(&bench.events()));
}
