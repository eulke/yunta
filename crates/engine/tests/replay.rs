use yunta_core::events::{
    Event, EventPayload, NodeFailedPayload, NodeFinishedPayload, NodeStartedPayload, TaskStatus,
    TaskStatusChangedPayload, TokenUsage,
};
use yunta_core::events::{RunPausedPayload, TaskRegisteredPayload};
use yunta_engine::{derive, NodeState};

fn event(seq: u64, node_id: Option<&str>, payload: EventPayload) -> Event {
    Event {
        run_id: "run-1".into(),
        seq,
        timestamp: chrono::Utc::now(),
        node_id: node_id.map(Into::into),
        payload,
    }
}

fn tokens(input: u64, output: u64) -> TokenUsage {
    TokenUsage {
        input,
        output,
        cached: None,
    }
}

#[test]
fn a_node_that_finishes_cleanly_derives_finished_with_its_tokens() {
    let events = vec![
        event(
            1,
            Some("lint"),
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
        ),
        event(
            2,
            Some("lint"),
            EventPayload::NodeFinished(NodeFinishedPayload {
                outcome: "criteria green".to_string(),
                tokens_used: tokens(10, 5),
            }),
        ),
    ];

    let state = derive(&events);
    assert_eq!(state.broken, None);
    assert_eq!(
        state.nodes.get(&"lint".into()),
        Some(&NodeState::Finished {
            outcome: "criteria green".to_string(),
            tokens: tokens(10, 5),
        })
    );
    assert_eq!(state.total_tokens, tokens(10, 5));
}

#[test]
fn a_retryable_failure_can_restart_and_then_finish() {
    let events = vec![
        event(
            1,
            Some("lint"),
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
        ),
        event(
            2,
            Some("lint"),
            EventPayload::NodeFailed(NodeFailedPayload {
                outcome: "criteria red".to_string(),
                tokens_used: tokens(5, 2),
                retryable: true,
            }),
        ),
        event(
            3,
            Some("lint"),
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 2 }),
        ),
        event(
            4,
            Some("lint"),
            EventPayload::NodeFinished(NodeFinishedPayload {
                outcome: "criteria green".to_string(),
                tokens_used: tokens(3, 1),
            }),
        ),
    ];

    let state = derive(&events);
    assert_eq!(state.broken, None);
    assert_eq!(
        state.nodes.get(&"lint".into()),
        Some(&NodeState::Finished {
            outcome: "criteria green".to_string(),
            tokens: tokens(3, 1),
        })
    );
    // Both the failed attempt and the finishing one count toward the total.
    assert_eq!(state.total_tokens, tokens(8, 3));
}

#[test]
fn node_finished_without_a_prior_node_started_is_broken() {
    let events = vec![event(
        1,
        Some("lint"),
        EventPayload::NodeFinished(NodeFinishedPayload {
            outcome: "criteria green".to_string(),
            tokens_used: tokens(1, 1),
        }),
    )];

    let state = derive(&events);
    let diagnostic = state.broken.expect("expected a broken diagnostic");
    assert!(diagnostic.contains("seq 1"));
    assert!(diagnostic.contains("lint"));
}

#[test]
fn a_second_node_started_is_a_restart_not_a_broken_log() {
    // §8.1 restart_node: a crash leaves node_started with no terminal
    // event, and resume emits node_started again. The log records what
    // happened — the restart is legal and the attempt number carries it.
    let events = vec![
        event(
            1,
            Some("lint"),
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
        ),
        event(
            2,
            Some("lint"),
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 2 }),
        ),
    ];

    let state = derive(&events);
    assert!(state.broken.is_none());
    assert!(matches!(
        state.nodes.get(&"lint".into()),
        Some(yunta_engine::NodeState::Running { attempt: 2 })
    ));
}

#[test]
fn task_status_changed_without_task_registered_is_broken() {
    let events = vec![event(
        1,
        Some("implement"),
        EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
            task_id: "graph-cmd".into(),
            new_status: TaskStatus::Done,
            caused_by: 0,
        }),
    )];

    let state = derive(&events);
    assert!(state.broken.is_some());
}

#[test]
fn task_lifecycle_derives_its_latest_status() {
    let events = vec![
        event(
            1,
            Some("implement"),
            EventPayload::TaskRegistered(TaskRegisteredPayload {
                task_id: "graph-cmd".into(),
                criteria: vec![],
                scope: vec!["crates/cli/**".to_string()],
                depends_on: vec![],
            }),
        ),
        event(
            2,
            Some("implement"),
            EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                task_id: "graph-cmd".into(),
                new_status: TaskStatus::Running,
                caused_by: 1,
            }),
        ),
        event(
            3,
            Some("implement"),
            EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                task_id: "graph-cmd".into(),
                new_status: TaskStatus::Done,
                caused_by: 2,
            }),
        ),
    ];

    let state = derive(&events);
    assert_eq!(state.broken, None);
    assert_eq!(
        state.tasks.get(&"graph-cmd".into()),
        Some(&TaskStatus::Done)
    );
}

#[test]
fn replay_stops_deriving_further_state_once_broken() {
    let events = vec![
        // broken immediately: no prior node_started for "lint"
        event(
            1,
            Some("lint"),
            EventPayload::NodeFinished(NodeFinishedPayload {
                outcome: "criteria green".to_string(),
                tokens_used: tokens(1, 1),
            }),
        ),
        // a perfectly valid event that comes after the break point
        event(
            2,
            None,
            EventPayload::RunPaused(RunPausedPayload {
                reason: "irrelevant".to_string(),
            }),
        ),
        event(
            3,
            Some("implement"),
            EventPayload::TaskRegistered(TaskRegisteredPayload {
                task_id: "graph-cmd".into(),
                criteria: vec![],
                scope: vec![],
                depends_on: vec![],
            }),
        ),
    ];

    let state = derive(&events);
    assert!(state.broken.is_some());
    assert!(
        state.tasks.is_empty(),
        "no event past the break point should be applied"
    );
}

#[test]
fn replay_is_deterministic_across_several_fixtures() {
    let fixtures: Vec<Vec<Event>> = vec![
        vec![
            event(
                1,
                Some("a"),
                EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
            ),
            event(
                2,
                Some("a"),
                EventPayload::NodeFinished(NodeFinishedPayload {
                    outcome: "ok".to_string(),
                    tokens_used: tokens(1, 1),
                }),
            ),
        ],
        vec![
            event(
                1,
                Some("a"),
                EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
            ),
            event(
                2,
                Some("a"),
                EventPayload::NodeFailed(NodeFailedPayload {
                    outcome: "bad".to_string(),
                    tokens_used: tokens(2, 2),
                    retryable: false,
                }),
            ),
            event(
                3,
                Some("b"),
                EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
            ),
        ],
        vec![event(
            1,
            Some("a"),
            EventPayload::NodeFinished(NodeFinishedPayload {
                outcome: "broken from the start".to_string(),
                tokens_used: tokens(0, 0),
            }),
        )],
    ];

    for events in fixtures {
        let first = derive(&events);
        let second = derive(&events);
        assert_eq!(first, second);
    }
}
