use std::sync::Arc;
use std::thread;

use yunta_core::events::{EventBody, EventDraft, EventPayload, EventShapeError, RunPausedPayload};
use yunta_core::{RunId, Seq};
use yunta_storage::{AsyncStorage, ChainVerification, Purge, Storage, StorageError};

fn open_temp() -> (tempfile::TempDir, Storage) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(&dir.path().join("yunta.db")).unwrap();
    (dir, storage)
}

#[test]
fn append_then_read_round_trips() {
    let (_dir, storage) = open_temp();
    let run_id = RunId::from("run-1");

    storage
        .append(&created_draft("run-1"), &yunta_core::SystemClock)
        .unwrap();
    let seq = storage
        .append(
            &paused_draft("run-1", "budget exceeded"),
            &yunta_core::SystemClock,
        )
        .unwrap();
    assert_eq!(seq.get(), 2);

    let events = storage.events_for_run(&run_id).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].seq.get(), 2);
    match events[1].payload() {
        Some(EventPayload::RunPaused(p)) => assert_eq!(p.reason, "budget exceeded"),
        other => panic!("expected RunPaused, got {other:?}"),
    }
}

#[test]
fn seq_is_assigned_monotonically_per_run_and_replay_is_ordered() {
    let (_dir, storage) = open_temp();
    let run_id = RunId::from("run-1");

    storage
        .append(&created_draft("run-1"), &yunta_core::SystemClock)
        .unwrap();
    for i in 0..5 {
        let seq = storage
            .append(
                &paused_draft("run-1", &format!("reason-{i}")),
                &yunta_core::SystemClock,
            )
            .unwrap();
        assert_eq!(seq.get(), i + 2);
    }

    let events = storage.events_for_run(&run_id).unwrap();
    let seqs: Vec<u64> = events.iter().map(|e| e.seq.get()).collect();
    assert_eq!(seqs, vec![1, 2, 3, 4, 5, 6]);
}

#[test]
fn different_runs_get_independent_sequences() {
    let (_dir, storage) = open_temp();

    storage
        .append(&created_draft("run-a"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(&created_draft("run-b"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(&paused_draft("run-a", "a1"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(&paused_draft("run-b", "b1"), &yunta_core::SystemClock)
        .unwrap();
    let seq_a2 = storage
        .append(&paused_draft("run-a", "a2"), &yunta_core::SystemClock)
        .unwrap();

    assert_eq!(seq_a2.get(), 3);
    assert_eq!(
        storage.events_for_run(&RunId::from("run-b")).unwrap().len(),
        2
    );
}

#[test]
fn concurrent_appends_never_lose_or_collide_a_seq() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(Storage::open(&dir.path().join("yunta.db")).unwrap());
    let run_id = RunId::from("run-concurrent");
    storage
        .append(&created_draft("run-concurrent"), &yunta_core::SystemClock)
        .unwrap();

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let storage = Arc::clone(&storage);
            thread::spawn(move || {
                storage
                    .append(
                        &paused_draft("run-concurrent", &format!("writer-{i}")),
                        &yunta_core::SystemClock,
                    )
                    .unwrap()
            })
        })
        .collect();

    let mut assigned_seqs: Vec<u64> = handles
        .into_iter()
        .map(|h| h.join().unwrap().get())
        .collect();
    assigned_seqs.sort_unstable();
    assert_eq!(assigned_seqs, vec![2, 3, 4, 5, 6, 7, 8, 9]);

    let events = storage.events_for_run(&run_id).unwrap();
    assert_eq!(events.len(), 9);
    let stored_seqs: Vec<u64> = events.iter().map(|e| e.seq.get()).collect();
    assert_eq!(stored_seqs, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
}

/// The production shape a shared-handle test can never exercise — the
/// per-session run-tools listener appends `finding_posted` through its
/// own connection while the engine appends session/audit events through
/// another, both into the same run, at the same time. `append_event`'s transaction reads (`MAX(seq)`)
/// before writing; under a deferred `BEGIN`, that read→write upgrade
/// can't wait on a busy writer (SQLite refuses to invoke the busy
/// handler on an upgrade — waiting could deadlock) and surfaces
/// `SQLITE_BUSY` immediately, `busy_timeout` notwithstanding. `BEGIN
/// IMMEDIATE` takes the write lock up front, where the busy handler
/// does apply.
#[test]
fn concurrent_appends_across_two_connections_never_fail_busy() {
    let (_dir, storage) = open_temp();
    let storage = Arc::new(storage);
    let second = Arc::new(Storage::open(storage.path()).unwrap());
    let run_id = RunId::from("run-two-conns");
    storage
        .append(&created_draft("run-two-conns"), &yunta_core::SystemClock)
        .unwrap();

    const PER_WRITER: usize = 50;
    let handles: Vec<_> = [Arc::clone(&storage), Arc::clone(&second)]
        .into_iter()
        .enumerate()
        .map(|(writer, handle)| {
            thread::spawn(move || {
                for i in 0..PER_WRITER {
                    handle
                        .append(
                            &paused_draft("run-two-conns", &format!("writer-{writer}-{i}")),
                            &yunta_core::SystemClock,
                        )
                        .unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }

    let events = storage.events_for_run(&run_id).unwrap();
    assert_eq!(events.len(), 1 + 2 * PER_WRITER);
    let seqs: Vec<u64> = events.iter().map(|e| e.seq.get()).collect();
    let expected: Vec<u64> = (1..=(1 + 2 * PER_WRITER) as u64).collect();
    assert_eq!(seqs, expected, "no gaps, no duplicates, both connections");
}

#[test]
fn list_runs_returns_every_distinct_run() {
    let (_dir, storage) = open_temp();
    storage
        .append(&created_draft("run-a"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(&created_draft("run-b"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(&paused_draft("run-a", "a2"), &yunta_core::SystemClock)
        .unwrap();

    let run_ids: Vec<RunId> = storage
        .list_runs()
        .unwrap()
        .into_iter()
        .map(|run| run.run_id)
        .collect();
    assert_eq!(run_ids, vec![RunId::from("run-a"), RunId::from("run-b")]);
}

#[test]
fn a_reopened_database_keeps_its_events() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("yunta.db");

    {
        let storage = Storage::open(&db_path).unwrap();
        storage
            .append(&created_draft("run-1"), &yunta_core::SystemClock)
            .unwrap();
        storage
            .append(
                &paused_draft("run-1", "first session"),
                &yunta_core::SystemClock,
            )
            .unwrap();
    }

    let storage = Storage::open(&db_path).unwrap();
    let events = storage.events_for_run(&RunId::from("run-1")).unwrap();
    assert_eq!(events.len(), 2);
}

#[test]
fn a_corrupted_payload_surfaces_as_a_typed_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("yunta.db");
    let storage = Storage::open(&db_path).unwrap();
    storage
        .append(&created_draft("run-1"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "fine"), &yunta_core::SystemClock)
        .unwrap();

    // Reach past the crate's own interface to corrupt the row directly —
    // this simulates disk-level corruption, which is exactly the
    // scenario `events_for_run` must report as a typed error, never a
    // panic.
    let raw = rusqlite::Connection::open(&db_path).unwrap();
    raw.execute(
        "UPDATE events SET payload_json = 'not json' WHERE seq = 1",
        [],
    )
    .unwrap();

    let result = storage.events_for_run(&RunId::from("run-1"));
    assert!(matches!(result, Err(StorageError::CorruptPayload { .. })));
}

// --- event hash chain ---------------------------------------------------

#[test]
fn an_untouched_log_verifies_intact() {
    let (_dir, storage) = open_temp();
    let run_id = RunId::from("run-1");
    storage
        .append(&created_draft("run-1"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(
            &paused_draft("run-1", "first pause"),
            &yunta_core::SystemClock,
        )
        .unwrap();
    storage
        .append(
            &paused_draft("run-1", "second pause"),
            &yunta_core::SystemClock,
        )
        .unwrap();

    match storage.verify_chain(&run_id).unwrap() {
        ChainVerification::Intact { events } => assert_eq!(events, 3),
        other => panic!("expected an intact chain, got {other:?}"),
    }
}

#[test]
fn a_tampered_payload_breaks_the_chain_at_that_exact_seq() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("yunta.db");
    let storage = Storage::open(&db).unwrap();
    let run_id = RunId::from("run-1");
    storage
        .append(&created_draft("run-1"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "honest"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "later"), &yunta_core::SystemClock)
        .unwrap();
    drop(storage);

    // Tamper behind the store's back — exactly what the chain exists to
    // catch: integrity, not authenticity.
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE events SET payload_json = replace(payload_json, 'honest', 'edited') \
         WHERE run_id = 'run-1' AND seq = 2",
        [],
    )
    .unwrap();
    drop(conn);

    let storage = Storage::open(&db).unwrap();
    match storage.verify_chain(&run_id).unwrap() {
        ChainVerification::Broken { seq, detail } => {
            assert_eq!(seq.get(), 2);
            assert!(!detail.is_empty());
        }
        other => panic!("expected a broken chain at seq 2, got {other:?}"),
    }
}

#[test]
fn a_deleted_event_breaks_the_chain_where_the_gap_starts() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("yunta.db");
    let storage = Storage::open(&db).unwrap();
    let run_id = RunId::from("run-1");
    storage
        .append(&created_draft("run-1"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "a"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "b"), &yunta_core::SystemClock)
        .unwrap();
    drop(storage);

    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute("DELETE FROM events WHERE run_id = 'run-1' AND seq = 2", [])
        .unwrap();
    drop(conn);

    let storage = Storage::open(&db).unwrap();
    match storage.verify_chain(&run_id).unwrap() {
        ChainVerification::Broken { seq, detail } => {
            assert_eq!(seq.get(), 3, "the gap is visible where seq jumps");
            assert!(!detail.is_empty());
        }
        other => panic!("expected a broken chain, got {other:?}"),
    }
}

#[test]
fn a_run_whose_first_event_is_not_run_created_is_refused_at_append() {
    let (_dir, storage) = open_temp();
    let err = storage
        .append(
            &paused_draft("run-genesis", "no birth certificate"),
            &yunta_core::SystemClock,
        )
        .unwrap_err();
    assert!(
        matches!(err, StorageError::GenesisMissing { .. }),
        "got: {err:?}"
    );
}

#[test]
fn verifying_an_unknown_run_is_an_error_not_a_vacuous_intact() {
    let (_dir, storage) = open_temp();
    assert!(storage.verify_chain(&RunId::from("run-ghost")).is_err());
}

// --- database-level retention ---------------------------------------------

#[test]
fn purge_run_removes_every_row_for_exactly_that_run() {
    let (_dir, storage) = open_temp();
    storage
        .append(&created_draft("run-old"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(&paused_draft("run-old", "done"), &yunta_core::SystemClock)
        .unwrap();
    storage
        .append(&created_draft("run-live"), &yunta_core::SystemClock)
        .unwrap();

    let purged = storage.purge_run(&RunId::from("run-old")).unwrap();
    assert_eq!(purged.rows, 2);

    assert!(
        storage
            .events_for_run(&RunId::from("run-old"))
            .unwrap()
            .is_empty(),
        "a purged run reads back as unknown, never as corrupt state"
    );
    assert_eq!(
        storage
            .list_runs()
            .unwrap()
            .into_iter()
            .map(|run| run.run_id)
            .collect::<Vec<_>>(),
        vec![RunId::from("run-live")],
        "other runs' rows are untouched"
    );
}

// --- a second connection interleaves writes safely ------------------------

#[test]
fn a_second_connection_appends_interleaved_with_the_first_and_seq_stays_monotonic() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(&dir.path().join("events.db")).unwrap();
    let second = Storage::open(storage.path()).unwrap();

    let run_id = RunId::from("run-reopen");
    let seq1 = storage
        .append(&created_draft("run-reopen"), &yunta_core::SystemClock)
        .unwrap();
    let mut event = created_draft("run-reopen");
    event.payload =
        yunta_core::events::EventPayload::RunPaused(yunta_core::events::RunPausedPayload {
            reason: "from the second handle".to_string(),
        });
    let seq2 = second.append(&event, &yunta_core::SystemClock).unwrap();

    assert_eq!((seq1.get(), seq2.get()), (1, 2));
    let events = storage.events_for_run(&run_id).unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[1].payload(),
        Some(yunta_core::events::EventPayload::RunPaused(_))
    ));
}

// --- drafts, stored events and the tolerant reader -------------------------

/// A clock that answers a fixed instant, so a stamped timestamp is checkable.
struct FixedClock(chrono::DateTime<chrono::Utc>);

impl yunta_core::Clock for FixedClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }
}

fn created_draft(run_id: &str) -> EventDraft {
    EventDraft {
        run_id: RunId::from(run_id),
        node_id: None,
        payload: EventPayload::RunCreated(yunta_core::events::RunCreatedPayload {
            manifest_hash: "abc123manifest".to_string(),
            inputs: Default::default(),
            mode: "default".into(),
            promoted_from: None,
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: "deadbeef".to_string(),
        }),
    }
}

fn paused_draft(run_id: &str, reason: &str) -> EventDraft {
    EventDraft {
        run_id: RunId::from(run_id),
        node_id: None,
        payload: EventPayload::RunPaused(RunPausedPayload {
            reason: reason.to_string(),
        }),
    }
}

#[test]
fn a_draft_is_stamped_by_the_injected_clock_and_gets_the_next_seq() {
    let (_dir, storage) = open_temp();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-09-02T10:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let clock = FixedClock(instant);

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
        Some(EventPayload::RunPaused(p)) => assert_eq!(p.reason, "budget"),
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
        FixedClock(
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

#[tokio::test]
async fn async_storage_appends_off_the_runtime_thread_and_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let storage = AsyncStorage::open(dir.path().join("yunta.db"))
        .await
        .unwrap();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-09-02T10:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let first = storage
        .append(created_draft("run-1"), instant)
        .await
        .unwrap();
    let second = storage
        .append(paused_draft("run-1", "budget"), instant)
        .await
        .unwrap();
    assert_eq!((first, second), (Seq::FIRST, Seq::FIRST.next()));

    let events = storage.events_for_run(RunId::from("run-1")).await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].timestamp, instant);
    assert!(matches!(
        storage.verify_chain(RunId::from("run-1")).await.unwrap(),
        ChainVerification::Intact { events: 2 }
    ));
    assert_eq!(storage.list_runs().await.unwrap().len(), 1);
    assert_eq!(
        storage.purge_run(RunId::from("run-1")).await.unwrap(),
        Purge { rows: 2 }
    );
}

#[tokio::test]
async fn async_storage_runs_a_compound_operation_on_one_blocking_connection() {
    let dir = tempfile::tempdir().unwrap();
    let storage = AsyncStorage::open(dir.path().join("yunta.db"))
        .await
        .unwrap();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-09-02T10:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    storage
        .append(created_draft("run-1"), instant)
        .await
        .unwrap();
    storage
        .append(created_draft("run-2"), instant)
        .await
        .unwrap();

    let run_ids = storage
        .blocking("collect run ids", |storage| {
            let mut ids: Vec<String> = Vec::new();
            for run in storage.list_runs()? {
                let events = storage.events_for_run(&run.run_id)?;
                ids.push(format!("{}:{}", run.run_id, events.len()));
            }
            Ok(ids)
        })
        .await
        .unwrap();
    assert_eq!(run_ids, vec!["run-1:1", "run-2:1"]);
}

#[tokio::test]
async fn async_storage_open_fails_on_a_path_that_cannot_be_opened() {
    let dir = tempfile::tempdir().unwrap();
    let error = AsyncStorage::open(dir.path().join("missing").join("yunta.db"))
        .await
        .unwrap_err();
    assert!(matches!(error, StorageError::Open { .. }), "{error:?}");
}

#[tokio::test]
async fn async_storage_reports_a_log_that_vanished_as_the_calls_error() {
    let dir = tempfile::tempdir().unwrap();
    let storage = AsyncStorage::open(dir.path().join("yunta.db"))
        .await
        .unwrap();
    std::fs::remove_dir_all(dir.path()).unwrap();
    let error = storage.list_runs().await.unwrap_err();
    assert!(matches!(error, StorageError::Open { .. }), "{error:?}");
}
