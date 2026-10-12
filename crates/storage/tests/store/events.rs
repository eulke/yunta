//! Drafts, stored events and the tolerant reader: a draft is stamped by the
//! injected clock, a kind this binary does not know is kept rather than
//! failing the read, and a known kind whose payload does not fit its shape
//! is corrupt.

use super::*;

#[test]
fn a_draft_is_stamped_by_the_injected_clock_and_gets_the_next_seq() {
    let (_dir, storage) = open_temp();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-09-02T10:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let clock = yunta_testkit_core::AtClock(instant);

    let first = storage.append(&created_draft("run-1"), &clock).unwrap();
    let second = storage
        .append(&paused_draft("run-1", "budget"), &clock)
        .unwrap();
    assert_eq!(first, Seq::FIRST);
    assert_eq!(second, Seq::FIRST.next());

    let events = storage.events_for_run(&RunId::from("run-1")).unwrap();
    assert_eq!(events[1].seq, second);
    assert_eq!(events[1].timestamp, instant);
    match events[1].payload() {
        Some(EventPayload::Run(RunEvent::Paused(p))) => assert_eq!(
            p.pause,
            PauseReason::Recorded {
                sentence: "budget".into()
            }
        ),
        other => panic!("expected run_paused, got {other:?}"),
    }
}

#[test]
fn unknown_kind_is_preserved_not_fatal() {
    let (dir, storage) = open_temp();
    storage
        .append(&created_draft("run-1"), &yunta_core::SystemClock)
        .unwrap();
    // A row a newer binary wrote: a kind this one does not know.
    let conn = rusqlite::Connection::open(dir.path().join("yunta.db")).unwrap();
    conn.execute(
        "INSERT INTO events (run_id, seq, ts, node_id, kind, payload_json, schema_version, \
         event_hash) VALUES ('run-1', 2, '2026-09-02T10:00:00+00:00', 'plan', 'future_kind', \
         '{\"kind\":\"future_kind\",\"novel\":true}', 3, 'unverified')",
        [],
    )
    .unwrap();

    let events = storage.events_for_run(&RunId::from("run-1")).unwrap();
    assert_eq!(events.len(), 2);
    let EventBody::Unknown(unknown) = &events[1].body else {
        panic!(
            "expected the unknown kind to be preserved, got {:?}",
            events[1].body
        );
    };
    assert_eq!(unknown.kind, "future_kind");
    assert_eq!(unknown.schema_version, 3);
    assert_eq!(unknown.payload["novel"], serde_json::Value::Bool(true));
    assert_eq!(events[1].seq.get(), 2);
    assert_eq!(
        events[1].node_id.as_ref().map(|id| id.as_str()),
        Some("plan")
    );
}

#[test]
fn a_known_kind_whose_payload_is_not_its_shape_is_corrupt_not_unknown() {
    let (dir, storage) = open_temp();
    storage
        .append(&created_draft("run-1"), &yunta_core::SystemClock)
        .unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("yunta.db")).unwrap();
    conn.execute(
        "INSERT INTO events (run_id, seq, ts, node_id, kind, payload_json, schema_version, \
         event_hash) VALUES ('run-1', 2, '2026-09-02T10:00:00+00:00', NULL, 'run_paused', \
         '{\"kind\":\"run_paused\"}', 1, 'unverified')",
        [],
    )
    .unwrap();
    let result = storage.events_for_run(&RunId::from("run-1"));
    assert!(
        matches!(
            result,
            Err(StorageError::CorruptPayload { ref seq, ref source, .. })
                if seq.get() == 2 && matches!(source, EventShapeError::Payload { kind, .. } if kind == "run_paused")
        ),
        "{result:?}"
    );
}

#[test]
fn runs_are_listed_by_their_first_timestamp() {
    let (_dir, storage) = open_temp();
    let at = |text: &str| {
        yunta_testkit_core::AtClock(
            chrono::DateTime::parse_from_rfc3339(text)
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
    };
    storage
        .append(&created_draft("run-late"), &at("2026-09-02T12:00:00+00:00"))
        .unwrap();
    storage
        .append(
            &created_draft("run-early"),
            &at("2026-09-02T09:00:00+00:00"),
        )
        .unwrap();
    storage
        .append(
            &paused_draft("run-early", "later"),
            &at("2026-09-02T13:00:00+00:00"),
        )
        .unwrap();

    let listed = storage.list_runs().unwrap();
    let ids: Vec<&str> = listed.iter().map(|run| run.run_id.as_str()).collect();
    assert_eq!(ids, vec!["run-early", "run-late"]);
    assert_eq!(
        listed[0].created_at,
        at("2026-09-02T09:00:00+00:00").0,
        "a run is dated by its first event, not its latest"
    );
}

#[test]
fn a_storage_error_keeps_its_cause_without_naming_the_backend() {
    let dir = tempfile::tempdir().unwrap();
    let error = Storage::open(dir.path()).unwrap_err();
    let StorageError::Open { source, .. } = &error else {
        panic!("expected an open error, got {error:?}");
    };
    let cause: &(dyn std::error::Error + Send + Sync) = source.as_ref();
    assert!(!cause.to_string().is_empty());
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn purging_a_run_reports_the_rows_it_removed() {
    let (_dir, storage) = open_temp();
    storage
        .append(&created_draft("run-old"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(&paused_draft("run-old", "x"), &yunta_core::SystemClock)
        .unwrap();
    assert_eq!(
        storage.purge_run(&RunId::from("run-old")).unwrap(),
        Purge { rows: 2 }
    );
    assert_eq!(
        storage.purge_run(&RunId::from("run-old")).unwrap(),
        Purge { rows: 0 }
    );
}
