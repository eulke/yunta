use yunta_core::events::{
    EventBody, EventPayload, Failure, Finding, FindingPostedPayload, FindingSeverity,
    NodeFailedPayload, NodeFinishedPayload, NodeStartedPayload, StoredEvent, TaskStatus,
    TaskStatusChangedPayload, TokenUsage, UnknownEvent,
};
use yunta_core::events::{RunPausedPayload, TaskRegisteredPayload};
use yunta_core::Seq;
use yunta_engine::{dedup_findings, derive, NodeState};

fn finding(id: &str, severity: FindingSeverity, title: &str, location: &str) -> Finding {
    Finding {
        id: id.into(),
        severity,
        title: title.to_string(),
        location: location.to_string(),
        detail: "detail".to_string(),
        proposed_criterion: None,
    }
}

fn event(seq: u64, node_id: Option<&str>, payload: EventPayload) -> StoredEvent {
    StoredEvent {
        run_id: "run-1".into(),
        seq: seq.into(),
        timestamp: chrono::Utc::now(),
        node_id: node_id.map(Into::into),
        body: EventBody::Known(payload),
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
        state.nodes.get("lint"),
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
            EventPayload::NodeFailed(NodeFailedPayload::new(
                Failure::message("criteria red".to_string()),
                true,
                tokens(5, 2),
            )),
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
        state.nodes.get("lint"),
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
    assert_eq!(
        diagnostic,
        "seq 1: node `lint` got node_finished without a matching node_started"
    );
}

#[test]
fn a_second_node_started_is_a_restart_not_a_broken_log() {
    // restart_node: a crash leaves node_started with no terminal
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
        state.nodes.get("lint"),
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
            caused_by: 1.into(),
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
                caused_by: 1.into(),
            }),
        ),
        event(
            3,
            Some("implement"),
            EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                task_id: "graph-cmd".into(),
                new_status: TaskStatus::Done,
                caused_by: 2.into(),
            }),
        ),
    ];

    let state = derive(&events);
    assert_eq!(state.broken, None);
    assert_eq!(state.tasks.get("graph-cmd"), Some(&TaskStatus::Done));
}

#[test]
fn finding_posted_events_accumulate_in_run_state() {
    let events = vec![
        event(
            1,
            Some("review"),
            EventPayload::FindingPosted(FindingPostedPayload {
                finding: finding(
                    "f1",
                    FindingSeverity::Major,
                    "unchecked error",
                    "src/lib.rs:10",
                ),
            }),
        ),
        event(
            2,
            Some("review"),
            EventPayload::FindingPosted(FindingPostedPayload {
                finding: finding("f2", FindingSeverity::Note, "style nit", "src/lib.rs:20"),
            }),
        ),
    ];

    let state = derive(&events);
    assert_eq!(state.broken, None);
    assert_eq!(state.findings.len(), 2);
    assert_eq!(state.findings[0].id, "f1");
    assert_eq!(state.findings[1].id, "f2");
}

#[test]
fn dedup_findings_merges_same_location_and_normalized_title_keeping_the_first() {
    let findings = vec![
        finding(
            "reviewer-a-1",
            FindingSeverity::Major,
            "Unchecked Error",
            "src/lib.rs:10",
        ),
        finding(
            "reviewer-b-1",
            FindingSeverity::Major,
            "unchecked error", // same title, different case
            "src/lib.rs:10",   // same location
        ),
        finding(
            "reviewer-a-2",
            FindingSeverity::Minor,
            "different finding",
            "src/lib.rs:10",
        ),
    ];

    let deduped = dedup_findings(&findings);
    assert_eq!(deduped.len(), 2);
    assert_eq!(deduped[0].id, "reviewer-a-1", "keeps the first occurrence");
    assert_eq!(deduped[1].id, "reviewer-a-2");
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
    let fixtures: Vec<Vec<StoredEvent>> = vec![
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
                EventPayload::NodeFailed(NodeFailedPayload::new(
                    Failure::message("bad".to_string()),
                    false,
                    tokens(2, 2),
                )),
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

#[test]
fn an_unknown_kind_is_counted_and_never_breaks_replay() {
    let unknown = StoredEvent {
        run_id: "run-1".into(),
        seq: 2.into(),
        timestamp: chrono::Utc::now(),
        node_id: Some("lint".into()),
        body: EventBody::Unknown(UnknownEvent {
            kind: "future_kind".to_string(),
            schema_version: 1,
            payload: serde_json::Map::new(),
        }),
    };
    let events = vec![
        event(
            1,
            Some("lint"),
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
        ),
        unknown,
        event(
            3,
            Some("lint"),
            EventPayload::NodeFinished(NodeFinishedPayload {
                outcome: "criteria green".to_string(),
                tokens_used: tokens(10, 5),
            }),
        ),
    ];

    let state = derive(&events);
    assert_eq!(state.broken, None);
    assert!(matches!(
        state.nodes.get("lint"),
        Some(NodeState::Finished { .. })
    ));
    assert_eq!(
        state.unknown_kinds,
        vec![(Seq::try_from(2_i64).unwrap(), "future_kind".to_string())]
    );
}
