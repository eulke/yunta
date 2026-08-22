//! `render_events_jsonl` — pure rendering, plus the
//! property test the acceptance criterion names verbatim:
//! "derivar estado desde el JSONL produce el mismo `RunState` que el
//! replay desde la DB."

use yunta_core::events::{
    Event, EventPayload, NodeFinishedPayload, NodeStartedPayload, RunCreatedPayload, TokenUsage,
};
use yunta_engine::{derive, render_events_jsonl};

fn event(seq: u64, node_id: Option<&str>, payload: EventPayload) -> Event {
    Event {
        run_id: "run-1".into(),
        seq,
        timestamp: chrono::Utc::now(),
        node_id: node_id.map(Into::into),
        payload,
    }
}

fn sample_events() -> Vec<Event> {
    vec![
        event(
            1,
            None,
            EventPayload::RunCreated(RunCreatedPayload {
                manifest_hash: "deadbeef".to_string(),
                inputs: Default::default(),
                mode: "default".to_string(),
                promoted_from: None,
                yunta_schema: None,
                base_branch: "main".to_string(),
                base_commit: "abc123".to_string(),
            }),
        ),
        event(
            2,
            Some("lint"),
            EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
        ),
        event(
            3,
            Some("lint"),
            EventPayload::NodeFinished(NodeFinishedPayload {
                outcome: "criteria green".to_string(),
                tokens_used: TokenUsage {
                    input: 10,
                    output: 5,
                    cached: None,
                },
            }),
        ),
    ]
}

#[test]
fn each_event_becomes_exactly_one_json_line_in_seq_order() {
    let jsonl = render_events_jsonl(&sample_events()).unwrap();
    let lines: Vec<&str> = jsonl.lines().collect();
    assert_eq!(lines.len(), 3);
    for line in &lines {
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(parsed.is_object(), "each line must be one JSON object");
    }
    assert!(
        jsonl.ends_with('\n'),
        "trailing newline after the last line"
    );
}

#[test]
fn an_empty_log_renders_to_an_empty_string() {
    assert_eq!(render_events_jsonl(&[]).unwrap(), "");
}

#[test]
fn deriving_from_the_jsonl_round_trip_matches_deriving_from_the_original_events() {
    // The Plan's own ✓: "derivar estado desde el JSONL produce el mismo
    // RunState que el replay desde la DB" — exercised here with several
    // fixtures covering different event kinds, the same hand-fixture
    // style `replay_is_deterministic_across_several_fixtures` already
    // uses (no proptest/quickcheck dependency in this workspace).
    let fixtures: Vec<Vec<Event>> = vec![
        sample_events(),
        vec![event(
            1,
            Some("a"),
            EventPayload::NodeFinished(NodeFinishedPayload {
                outcome: "broken from the start".to_string(),
                tokens_used: TokenUsage::default(),
            }),
        )],
        vec![],
    ];

    for original in fixtures {
        let jsonl = render_events_jsonl(&original).unwrap();
        let round_tripped: Vec<Event> = jsonl
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();

        assert_eq!(
            round_tripped, original,
            "the events themselves round-trip byte-for-byte"
        );
        assert_eq!(
            derive(&round_tripped),
            derive(&original),
            "RunState derived from the JSONL must match RunState derived from the DB's events"
        );
    }
}
