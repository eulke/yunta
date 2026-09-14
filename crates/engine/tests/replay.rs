use yunta_core::events::{
    ArtifactAcceptedPayload, ArtifactId, ArtifactOrigin, ArtifactWrittenPayload, EventBody,
    EventPayload, Failure, Finding, FindingPostedPayload, FindingSeverity, NodeFailedPayload,
    NodeFinishedPayload, NodeStartedPayload, RecordedOrigin, StoredEvent, TaskStatus,
    TaskStatusChangedPayload, TokenUsage, UnknownEvent,
};
use yunta_core::events::{ArtifactEvent, FindingEvent, NodeEvent, RunEvent, TaskEvent};
use yunta_core::events::{RunPausedPayload, TaskRegisteredPayload};
use yunta_core::Seq;
use yunta_engine::{dedup_findings, derive, NodeState};

fn finding(id: &str, severity: FindingSeverity, title: &str, location: &str) -> Finding {
    Finding {
        id: id.into(),
        severity,
        title: title.to_string(),
        location: location.into(),
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
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        event(
            2,
            Some("lint"),
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                "criteria green".to_string(),
                tokens(10, 5),
            ))),
        ),
    ];

    let state = derive(&events);
    assert_eq!(state.broken, None);
    assert_eq!(
        state.nodes.state("lint"),
        Some(&NodeState::Finished {
            outcome: "criteria green".to_string(),
            tokens: tokens(10, 5),
        })
    );
    assert_eq!(state.total_tokens(), tokens(10, 5));
}

#[test]
fn a_retryable_failure_can_restart_and_then_finish() {
    let events = vec![
        event(
            1,
            Some("lint"),
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        event(
            2,
            Some("lint"),
            EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
                Failure::message("criteria red".to_string()),
                true,
                tokens(5, 2),
            ))),
        ),
        event(
            3,
            Some("lint"),
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(2))),
        ),
        event(
            4,
            Some("lint"),
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                "criteria green".to_string(),
                tokens(3, 1),
            ))),
        ),
    ];

    let state = derive(&events);
    assert_eq!(state.broken, None);
    assert_eq!(
        state.nodes.state("lint"),
        Some(&NodeState::Finished {
            outcome: "criteria green".to_string(),
            tokens: tokens(3, 1),
        })
    );
    // Both the failed attempt and the finishing one count toward the total.
    assert_eq!(state.total_tokens(), tokens(8, 3));
}

#[test]
fn node_finished_without_a_prior_node_started_is_broken() {
    let events = vec![event(
        1,
        Some("lint"),
        EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
            "criteria green".to_string(),
            tokens(1, 1),
        ))),
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
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        event(
            2,
            Some("lint"),
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(2))),
        ),
    ];

    let state = derive(&events);
    assert!(state.broken.is_none());
    assert!(matches!(
        state.nodes.state("lint"),
        Some(yunta_engine::NodeState::Running { attempt: 2 })
    ));
}

#[test]
fn task_status_changed_without_task_registered_is_broken() {
    let events = vec![event(
        1,
        Some("implement"),
        EventPayload::Tasks(TaskEvent::StatusChanged(TaskStatusChangedPayload::to(
            "graph-cmd".into(),
            TaskStatus::Done,
            1.into(),
        ))),
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
            EventPayload::Tasks(TaskEvent::Registered(TaskRegisteredPayload {
                task_id: "graph-cmd".into(),
                criteria: vec![],
                scope: vec!["crates/cli/**".into()],
                depends_on: vec![],
            })),
        ),
        event(
            2,
            Some("implement"),
            EventPayload::Tasks(TaskEvent::StatusChanged(TaskStatusChangedPayload::to(
                "graph-cmd".into(),
                TaskStatus::Running,
                1.into(),
            ))),
        ),
        event(
            3,
            Some("implement"),
            EventPayload::Tasks(TaskEvent::StatusChanged(TaskStatusChangedPayload::to(
                "graph-cmd".into(),
                TaskStatus::Done,
                2.into(),
            ))),
        ),
    ];

    let state = derive(&events);
    assert_eq!(state.broken, None);
    assert_eq!(state.tasks.status("graph-cmd"), Some(TaskStatus::Done));
}

#[test]
fn finding_posted_events_accumulate_in_run_state() {
    let events = vec![
        event(
            1,
            Some("review"),
            EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                finding: finding(
                    "f1",
                    FindingSeverity::Major,
                    "unchecked error",
                    "src/lib.rs:10",
                ),
            })),
        ),
        event(
            2,
            Some("review"),
            EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
                finding: finding("f2", FindingSeverity::Note, "style nit", "src/lib.rs:20"),
            })),
        ),
    ];

    let state = derive(&events);
    assert_eq!(state.broken, None);
    assert_eq!(state.effective_findings().len(), 2);
    assert_eq!(state.effective_findings()[0].id, "f1");
    assert_eq!(state.effective_findings()[1].id, "f2");
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
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                "criteria green".to_string(),
                tokens(1, 1),
            ))),
        ),
        // a perfectly valid event that comes after the break point
        event(
            2,
            None,
            EventPayload::Run(RunEvent::Paused(RunPausedPayload::recorded(
                "irrelevant".to_string(),
            ))),
        ),
        event(
            3,
            Some("implement"),
            EventPayload::Tasks(TaskEvent::Registered(TaskRegisteredPayload {
                task_id: "graph-cmd".into(),
                criteria: vec![],
                scope: vec![],
                depends_on: vec![],
            })),
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
                EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
            ),
            event(
                2,
                Some("a"),
                EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                    "ok".to_string(),
                    tokens(1, 1),
                ))),
            ),
        ],
        vec![
            event(
                1,
                Some("a"),
                EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
            ),
            event(
                2,
                Some("a"),
                EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
                    Failure::message("bad".to_string()),
                    false,
                    tokens(2, 2),
                ))),
            ),
            event(
                3,
                Some("b"),
                EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
            ),
        ],
        vec![event(
            1,
            Some("a"),
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                "broken from the start".to_string(),
                tokens(0, 0),
            ))),
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
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        ),
        unknown,
        event(
            3,
            Some("lint"),
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                "criteria green".to_string(),
                tokens(10, 5),
            ))),
        ),
    ];

    let state = derive(&events);
    assert_eq!(state.broken, None);
    assert!(matches!(
        state.nodes.state("lint"),
        Some(NodeState::Finished { .. })
    ));
    assert_eq!(
        state.unknown_kinds,
        vec![(Seq::try_from(2_i64).unwrap(), "future_kind".to_string())]
    );
}

#[test]
fn a_log_written_before_origins_derives_the_artifacts_a_newer_one_does() {
    // The same run twice: one log naming the file it wrote, one naming
    // the artifact it accepted. A `questions` artifact, because a node
    // that hands one over is the case where the two spellings most have
    // to agree about what the run holds.
    let hash = yunta_core::sha256_hex(b"questions");
    let run = |artifact: EventPayload| {
        vec![
            event(
                1,
                Some("ask"),
                EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
            ),
            event(2, Some("ask"), artifact),
            event(
                3,
                Some("ask"),
                EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
                    Failure::message("scope violated: 1 file(s) outside the declared globs"),
                    false,
                    tokens(0, 0),
                ))),
            ),
        ]
    };
    let old = run(EventPayload::Artifacts(ArtifactEvent::Written(
        ArtifactWrittenPayload {
            path: "artifacts/questions.yaml".into(),
            content_hash: hash.clone(),
            artifact_kind: Some(yunta_core::ArtifactKind::Questions),
        },
    )));
    let new = run(EventPayload::Artifacts(ArtifactEvent::Accepted(
        ArtifactAcceptedPayload::new(
            ArtifactId::Interpreted {
                kind: yunta_core::ArtifactKind::Questions,
            },
            hash.clone(),
            RecordedOrigin::Submitted,
        ),
    )));

    let (old, new) = (derive(&old), derive(&new));
    assert!(
        matches!(old.nodes.state("ask"), Some(NodeState::Failed { .. })),
        "a node that failed is failed, whatever documents the run holds for it: {:?}",
        old.nodes.state("ask")
    );
    // The states they derive, not the instants their events carry: the
    // two logs are written moments apart, and what this is about is the
    // artifacts each one implies.
    let states = |ledger: &yunta_core::events::NodeLedger| {
        ledger
            .iter()
            .map(|(id, record)| (id.clone(), record.state.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(states(&old.nodes), states(&new.nodes));

    let identities = |state: &yunta_engine::RunState| {
        state
            .artifacts
            .every()
            .map(|held| {
                (
                    held.producer.clone(),
                    held.artifact.clone(),
                    held.content_hash.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(identities(&old), identities(&new));
    assert_eq!(
        old.artifacts.every().next().unwrap().origin,
        ArtifactOrigin::Legacy,
        "the one thing an old log cannot state is how the run came by it"
    );
}

/// Every kind either moves state or says it does not. The pair is what
/// makes a silent no-op impossible: a kind nothing reads would sit in
/// the log deriving nothing, with no diagnostic anywhere — the exact
/// failure the wildcard used to allow.
#[test]
fn a_kind_that_moves_no_state_says_so_by_name() {
    let mut mismatched: Vec<String> = Vec::new();
    for payload in yunta_testkit_core::all_kinds() {
        let kind = payload.kind_name();
        let event = StoredEvent {
            run_id: yunta_core::RunId::from("run-audit"),
            seq: 1.into(),
            timestamp: chrono::DateTime::UNIX_EPOCH,
            node_id: Some("only".into()),
            body: yunta_core::events::EventBody::Known(payload.clone()),
        };
        // Each kind meets the log that makes it legal: a terminal needs
        // its start, a status its registration, an update the posting it
        // updates. Without one the derivation refuses the event, which
        // is a broken log rather than a no-op, and counts as moving.
        let mut state = yunta_engine::RunState::default();
        let before = {
            for setup in establishing() {
                // Never the kind under test: a log that already carries
                // it would make every repeat of it look like a no-op.
                if setup.kind_name() == kind {
                    continue;
                }
                let mut event = event.clone();
                event.body = yunta_core::events::EventBody::Known(setup);
                let _ = state.apply(&event);
            }
            state.clone()
        };
        let moved = match state.apply(&event) {
            // What a kind derives, not that its node was heard from:
            // every event of a node updates when it last said anything,
            // which is the envelope's doing and not the kind's.
            Ok(()) => state != before,
            Err(_) => true,
        };
        if !moved != payload.is_audit() {
            mismatched.push(format!(
                "{kind}: moves={moved} is_audit={}",
                payload.is_audit()
            ));
        }
    }
    assert!(
        mismatched.is_empty(),
        "every kind either moves a ledger or declares itself audit: {mismatched:#?}"
    );
}

/// The log every other kind needs behind it: a node that started, a task
/// that was registered, and a finding that was posted.
fn establishing() -> Vec<yunta_core::events::EventPayload> {
    yunta_testkit_core::all_kinds()
        .into_iter()
        .filter(|payload| {
            matches!(
                payload.kind_name(),
                "node_started" | "task_registered" | "finding_posted"
            )
        })
        .collect()
}
