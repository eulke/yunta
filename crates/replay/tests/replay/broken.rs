//! A log that breaks: replay stops at the first event it cannot take, and the
//! state up to that event still holds.

use super::*;

#[test]
fn replay_stops_deriving_further_state_once_broken() {
    let events = Log::for_run("run-1")
        // broken immediately: no prior node_started for "lint"
        .node(
            "lint",
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                NodeOutcome::Summary("criteria green".to_string()),
                tokens(1, 1),
            ))),
        )
        // a perfectly valid event that comes after the break point
        .event(EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
            &PauseReason::Recorded {
                sentence: "irrelevant".into(),
            },
        ))))
        .node(
            "implement",
            EventPayload::Tasks(TaskEvent::Registered(TaskRegisteredPayload {
                task_id: "graph-cmd".into(),
                criteria: vec![],
                scope: vec![],
                depends_on: vec![],
            })),
        )
        .build();

    let state = RunState::replay(&events);
    assert!(state.broken.is_some());
    assert!(
        state.tasks.is_empty(),
        "no event past the break point should be applied"
    );
}

/// A log that breaks reads up to the last event replay applied before the break, while its
/// head moves past it: what a run's screen says of how far it reads.
#[test]
fn a_broken_log_reads_up_to_the_last_event_applied_before_the_break() {
    let events = Log::for_run("run-1")
        .event(EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
            &PauseReason::Recorded {
                sentence: "made sense".into(),
            },
        ))))
        .node(
            "lint",
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                NodeOutcome::Summary("criteria green".to_string()),
                tokens(1, 1),
            ))),
        )
        .build();

    let state = RunState::replay(&events);
    assert!(state.broken.is_some());
    assert_eq!(state.reads_up_to().map(|seq| seq.get()), Some(1));
    assert_eq!(state.head().map(|seq| seq.get()), Some(2));
}
