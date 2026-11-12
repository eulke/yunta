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
        .append(&created_draft("run-1"), &FixedClock)
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
        .append(&created_draft("run-1"), &FixedClock)
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
        .append(&created_draft("run-old"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-old", "x"), &FixedClock)
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

/// What a reader that already folded a run up to `seq` has not seen: the log past `seq`, in
/// order, read the way the whole run is read.
#[test]
fn events_after_returns_what_events_for_run_returns_past_the_seq() {
    let (_dir, storage) = open_temp();
    let run_id = RunId::from("run-1");
    storage
        .append(&created_draft("run-1"), &FixedClock)
        .unwrap();
    for reason in ["a", "b", "c", "d"] {
        storage
            .append(&paused_draft("run-1", reason), &FixedClock)
            .unwrap();
    }
    storage
        .append(&created_draft("run-2"), &FixedClock)
        .unwrap();

    let whole = storage.events_for_run(&run_id).unwrap();
    for seq in whole.iter().map(|event| event.seq) {
        let past: Vec<_> = whole
            .iter()
            .filter(|event| event.seq > seq)
            .cloned()
            .collect();
        assert_eq!(storage.events_after(&run_id, seq).unwrap(), past, "{seq}");
    }
    let last = whole.last().unwrap().seq;
    assert!(storage.events_after(&run_id, last).unwrap().is_empty());
    let nobody = RunId::from("run-none");
    assert!(storage
        .events_after(&nobody, Seq::FIRST)
        .unwrap()
        .is_empty());
}

/// A kind a newer binary wrote, past the seq, comes back as it was written.
#[test]
fn events_after_keeps_an_unknown_kind_past_the_seq_verbatim() {
    let (dir, storage) = open_temp();
    storage
        .append(&created_draft("run-1"), &FixedClock)
        .unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("yunta.db")).unwrap();
    conn.execute(
        "INSERT INTO events (run_id, seq, ts, node_id, kind, payload_json, schema_version, \
         event_hash) VALUES ('run-1', 2, '2026-09-02T10:00:00+00:00', 'plan', 'future_kind', \
         '{\"kind\":\"future_kind\",\"novel\":true}', 3, 'unverified')",
        [],
    )
    .unwrap();

    let past = storage
        .events_after(&RunId::from("run-1"), Seq::FIRST)
        .unwrap();
    assert_eq!(past.len(), 1);
    let EventBody::Unknown(unknown) = &past[0].body else {
        panic!("the unknown kind is kept: {:?}", past[0].body);
    };
    assert_eq!(unknown.kind, "future_kind");
    assert_eq!(unknown.payload["novel"], serde_json::Value::Bool(true));
}

/// A corrupt row fails the read that reaches it, as it fails the whole run's, and a read
/// that starts past it never meets it.
#[test]
fn a_corrupt_row_fails_events_after_only_when_it_is_past_the_seq() {
    let (dir, storage) = open_temp();
    storage
        .append(&created_draft("run-1"), &FixedClock)
        .unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("yunta.db")).unwrap();
    conn.execute(
        "INSERT INTO events (run_id, seq, ts, node_id, kind, payload_json, schema_version, \
         event_hash) VALUES ('run-1', 2, '2026-09-02T10:00:00+00:00', NULL, 'run_paused', \
         '{\"kind\":\"run_paused\"}', 1, 'unverified')",
        [],
    )
    .unwrap();
    storage
        .append(&paused_draft("run-1", "after"), &FixedClock)
        .unwrap();
    let run_id = RunId::from("run-1");

    let reaching = storage.events_after(&run_id, Seq::FIRST);
    assert!(
        matches!(reaching, Err(StorageError::CorruptPayload { ref seq, .. }) if seq.get() == 2),
        "{reaching:?}"
    );
    let past = storage.events_after(&run_id, Seq::FIRST.next()).unwrap();
    assert_eq!(past.len(), 1);
    assert_eq!(past[0].seq.get(), 3);
}

/// The async store answers what the store answers.
#[tokio::test]
async fn async_events_after_answers_what_the_store_answers() {
    let (dir, storage) = open_temp();
    storage
        .append(&created_draft("run-1"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "a"), &FixedClock)
        .unwrap();
    let run_id = RunId::from("run-1");
    let handle = AsyncStorage::open(dir.path().join("yunta.db"))
        .await
        .unwrap();

    assert_eq!(
        handle
            .events_after(run_id.clone(), Seq::FIRST)
            .await
            .unwrap(),
        storage.events_after(&run_id, Seq::FIRST).unwrap()
    );
}
