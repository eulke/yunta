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

    let state = derive(&events);
    assert!(state.broken.is_some());
    assert!(
        state.tasks.is_empty(),
        "no event past the break point should be applied"
    );
}
