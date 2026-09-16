//! `render_events_jsonl` — pure rendering, plus the
//! property test the acceptance criterion names verbatim:
//! "derivar estado desde el JSONL produce el mismo `RunState` que el
//! replay desde la DB."

use yunta_core::events::{
    EventPayload, NodeFinishedPayload, NodeStartedPayload, RunCreatedPayload, StoredEvent,
    TokenUsage,
};
use yunta_core::events::{NodeEvent, RunEvent};
use yunta_engine::{derive, render_events_jsonl};
use yunta_testkit_core::Log;

fn sample_events() -> Vec<StoredEvent> {
    Log::for_run("run-1")
        .event(EventPayload::Run(RunEvent::Created(RunCreatedPayload {
            manifest_hash: yunta_core::sha256_hex(b"deadbeef"),
            inputs: Default::default(),
            mode: "default".into(),
            promoted_from: None,
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: "deadbeef".into(),
        })))
        .node(
            "lint",
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        )
        .node(
            "lint",
            EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                "criteria green".to_string(),
                TokenUsage {
                    input: 10,
                    output: 5,
                    cached: None,
                },
            ))),
        )
        .build()
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
    let fixtures: Vec<Vec<StoredEvent>> = vec![
        sample_events(),
        Log::for_run("run-1")
            .node(
                "a",
                EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                    "broken from the start".to_string(),
                    TokenUsage::default(),
                ))),
            )
            .build(),
        vec![],
    ];

    for original in fixtures {
        let jsonl = render_events_jsonl(&original).unwrap();
        let round_tripped: Vec<StoredEvent> = jsonl
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
