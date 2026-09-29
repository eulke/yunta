//! A run notices when the host it works on slept, and says so on its log
//! before anything that happened after.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use yunta_core::events::{EventPayload, NodeEvent, RunEvent, StoredEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{wait_until_async, Bench};
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
