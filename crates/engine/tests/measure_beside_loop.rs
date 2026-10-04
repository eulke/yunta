//! A loop starts beside the lineage's measurement. Its tasks carry the
//! suite as a guard from the start; a check whose own criteria pass waits
//! for the measurement there, and holds the task to the suite only when it
//! passed before the run changed anything.

use std::time::Duration;

use yunta_core::events::{EventPayload, NodeEvent, RunEvent, StoredEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{Bench, MOCK_CONFIG};

/// A plan of one task whose criterion its session's work makes pass, and
/// the loop that runs it.
const ONE_TASK: &str = r#"
name: beside
nodes:
  - id: plan
    kind: bash
    run: "printf 'tasks:\n  - id: T001\n    title: Write\n    scope: [a.txt]\n    criteria:\n      - cmd: test -f a.txt\n' > {{node.artifacts}}/tasks.yaml"
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

const WRITES: &str = "\
capabilities: { run_tools: true }
sessions:
  - effects:
      - { path: a.txt, content: a }
    outcome: { type: completed, summary: wrote }
";

/// Runs the loop with `suite` measured aside.
async fn beside(bench: &Bench, suite: &str) -> RunReport {
    let config = format!("{MOCK_CONFIG}baseline:\n  suite: \"{suite}\"\n");
    bench.run_with_config(ONE_TASK, WRITES, &config).await
}

/// Where in the log the first event `is` matches sits.
fn first(events: &[StoredEvent], is: impl Fn(&StoredEvent) -> bool) -> Option<usize> {
    events.iter().position(is)
}

fn measured(event: &StoredEvent) -> bool {
    matches!(
        event.payload(),
        Some(EventPayload::Run(RunEvent::BaselineCaptured(_)))
    )
}

/// The suite guard's answer in the task's check after its work, if any.
fn guard_after_the_work(events: &[StoredEvent], suite: &str) -> Option<usize> {
    first(events, |event| match event.payload() {
        Some(EventPayload::Node(NodeEvent::CriteriaChecked(check))) => {
            check.phase == yunta_core::events::Phase::Post
                && check.results.iter().any(|result| result.cmd == suite)
        }
        _ => false,
    })
}

#[tokio::test]
async fn a_loop_starts_before_the_aside_measurement_lands() {
    let bench = Bench::new();

    let RunReport { terminal, .. } = beside(&bench, "sleep 2; true").await;

    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    let started = first(&events, |event| {
        event
            .node_id
            .as_ref()
            .is_some_and(|node| node.as_str() == "implement")
            && matches!(
                event.payload(),
                Some(EventPayload::Node(NodeEvent::Started(_)))
            )
    });
    assert!(started < first(&events, measured), "the loop started first");
}

#[tokio::test]
async fn post_check_with_green_own_criteria_awaits_and_holds_the_guard() {
    let bench = Bench::new();

    let RunReport { terminal, .. } = beside(&bench, "sleep 2; true").await;

    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    let guard = guard_after_the_work(&events, "sleep 2; true");
    assert!(guard.is_some(), "the task was held to the suite");
    assert!(first(&events, measured) < guard, "after the suite answered");
}

#[tokio::test]
async fn a_red_measurement_holds_no_guard() {
    let bench = Bench::new();

    let RunReport { terminal, .. } = beside(&bench, "sleep 2; false").await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        guard_after_the_work(&bench.events(), "sleep 2; false"),
        None
    );
}

#[tokio::test]
async fn cancel_while_awaiting_pauses_cleanly() {
    let cancel = tokio_util::sync::CancellationToken::new();
    let bench = Bench::new().with_cancel(cancel.clone());
    let pool = bench.pool();
    // Tripped once the task's session wrote its work: its check is waiting.
    let trip = tokio::spawn(async move {
        let worked = || async {
            yunta_testkit::pool_checkouts_in(&pool)
                .iter()
                .any(|checkout| checkout.join("a.txt").exists())
                .then_some(())
        };
        yunta_testkit::wait_for_async(worked, || "the session never worked".into()).await;
        cancel.cancel();
    });

    let started = std::time::Instant::now();
    let RunReport { terminal, .. } = beside(&bench, "sleep 30; true").await;

    trip.await.unwrap();
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "it never waited the suite out"
    );
    assert_eq!(first(&bench.events(), measured), None);
}
