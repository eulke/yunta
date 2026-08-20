use std::sync::Arc;
use std::thread;

use yunta_core::events::{Event, EventPayload, RunPausedPayload};
use yunta_core::RunId;
use yunta_storage::{ChainVerification, Storage, StorageError};

fn open_temp() -> (tempfile::TempDir, Storage) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(&dir.path().join("yunta.db")).unwrap();
    (dir, storage)
}

fn paused_event(run_id: &str, reason: &str) -> Event {
    Event {
        run_id: RunId::from(run_id),
        seq: 0, // ignored by append_event — the store assigns the real seq
        timestamp: chrono::Utc::now(),
        node_id: None,
        payload: EventPayload::RunPaused(RunPausedPayload {
            reason: reason.to_string(),
        }),
    }
}

#[test]
fn append_then_read_round_trips() {
    let (_dir, storage) = open_temp();
    let run_id = RunId::from("run-1");

    storage.append_event(&created_event("run-1")).unwrap();
    let seq = storage
        .append_event(&paused_event("run-1", "budget exceeded"))
        .unwrap();
    assert_eq!(seq, 2);

    let events = storage.events_for_run(&run_id).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].seq, 2);
    match &events[1].payload {
        EventPayload::RunPaused(p) => assert_eq!(p.reason, "budget exceeded"),
        other => panic!("expected RunPaused, got {other:?}"),
    }
}

#[test]
fn seq_is_assigned_monotonically_per_run_and_replay_is_ordered() {
    let (_dir, storage) = open_temp();
    let run_id = RunId::from("run-1");

    storage.append_event(&created_event("run-1")).unwrap();
    for i in 0..5 {
        let seq = storage
            .append_event(&paused_event("run-1", &format!("reason-{i}")))
            .unwrap();
        assert_eq!(seq, i + 2);
    }

    let events = storage.events_for_run(&run_id).unwrap();
    let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![1, 2, 3, 4, 5, 6]);
}

#[test]
fn different_runs_get_independent_sequences() {
    let (_dir, storage) = open_temp();

    storage.append_event(&created_event("run-a")).unwrap();
    storage.append_event(&created_event("run-b")).unwrap();
    storage.append_event(&paused_event("run-a", "a1")).unwrap();
    storage.append_event(&paused_event("run-b", "b1")).unwrap();
    let seq_a2 = storage.append_event(&paused_event("run-a", "a2")).unwrap();

    assert_eq!(seq_a2, 3);
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
        .append_event(&created_event("run-concurrent"))
        .unwrap();

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let storage = Arc::clone(&storage);
            thread::spawn(move || {
                storage
                    .append_event(&paused_event("run-concurrent", &format!("writer-{i}")))
                    .unwrap()
            })
        })
        .collect();

    let mut assigned_seqs: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assigned_seqs.sort_unstable();
    assert_eq!(assigned_seqs, vec![2, 3, 4, 5, 6, 7, 8, 9]);

    let events = storage.events_for_run(&run_id).unwrap();
    assert_eq!(events.len(), 9);
    let stored_seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
    assert_eq!(stored_seqs, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
}

#[test]
fn list_run_ids_returns_every_distinct_run() {
    let (_dir, storage) = open_temp();
    storage.append_event(&created_event("run-a")).unwrap();
    storage.append_event(&created_event("run-b")).unwrap();
    storage.append_event(&paused_event("run-a", "a2")).unwrap();

    let run_ids = storage.list_run_ids().unwrap();
    assert_eq!(run_ids, vec![RunId::from("run-a"), RunId::from("run-b")]);
}

#[test]
fn a_reopened_database_keeps_its_events() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("yunta.db");

    {
        let storage = Storage::open(&db_path).unwrap();
        storage.append_event(&created_event("run-1")).unwrap();
        storage
            .append_event(&paused_event("run-1", "first session"))
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
    storage.append_event(&created_event("run-1")).unwrap();
    storage
        .append_event(&paused_event("run-1", "fine"))
        .unwrap();

    // Reach past the crate's own interface to corrupt the row directly —
    // this simulates disk-level corruption, which is exactly the
    // scenario `events_for_run` must report as a typed error (Contrato
    // §8.1's "broken" run), never a panic.
    let raw = rusqlite::Connection::open(&db_path).unwrap();
    raw.execute(
        "UPDATE events SET payload_json = 'not json' WHERE seq = 1",
        [],
    )
    .unwrap();

    let result = storage.events_for_run(&RunId::from("run-1"));
    assert!(matches!(result, Err(StorageError::CorruptPayload { .. })));
}

// --- T2.5: event hash chain (I26, docs/eventos.md §3) ------------------------

fn created_event(run_id: &str) -> Event {
    Event {
        run_id: RunId::from(run_id),
        seq: 0,
        timestamp: chrono::Utc::now(),
        node_id: None,
        payload: EventPayload::RunCreated(yunta_core::events::RunCreatedPayload {
            manifest_hash: "abc123manifest".to_string(),
            inputs: Default::default(),
            mode: "default".to_string(),
            promoted_from: None,
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: "deadbeef".to_string(),
        }),
    }
}

#[test]
fn an_untouched_log_verifies_intact() {
    let (_dir, storage) = open_temp();
    let run_id = RunId::from("run-1");
    storage.append_event(&created_event("run-1")).unwrap();
    storage
        .append_event(&paused_event("run-1", "first pause"))
        .unwrap();
    storage
        .append_event(&paused_event("run-1", "second pause"))
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
    storage.append_event(&created_event("run-1")).unwrap();
    storage
        .append_event(&paused_event("run-1", "honest"))
        .unwrap();
    storage
        .append_event(&paused_event("run-1", "later"))
        .unwrap();
    drop(storage);

    // Tamper behind the store's back — exactly what the chain exists to
    // catch (I26: integrity, not authenticity).
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
            assert_eq!(seq, 2);
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
    storage.append_event(&created_event("run-1")).unwrap();
    storage.append_event(&paused_event("run-1", "a")).unwrap();
    storage.append_event(&paused_event("run-1", "b")).unwrap();
    drop(storage);

    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute("DELETE FROM events WHERE run_id = 'run-1' AND seq = 2", [])
        .unwrap();
    drop(conn);

    let storage = Storage::open(&db).unwrap();
    match storage.verify_chain(&run_id).unwrap() {
        ChainVerification::Broken { seq, detail } => {
            assert_eq!(seq, 3, "the gap is visible where seq jumps");
            assert!(!detail.is_empty());
        }
        other => panic!("expected a broken chain, got {other:?}"),
    }
}

#[test]
fn a_run_whose_first_event_is_not_run_created_is_refused_at_append() {
    let (_dir, storage) = open_temp();
    let err = storage
        .append_event(&paused_event("run-genesis", "no birth certificate"))
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

// --- DI-14: database-level retention -----------------------------------------

#[test]
fn purge_run_removes_every_row_for_exactly_that_run() {
    let (_dir, storage) = open_temp();
    storage.append_event(&created_event("run-old")).unwrap();
    storage
        .append_event(&paused_event("run-old", "done"))
        .unwrap();
    storage.append_event(&created_event("run-live")).unwrap();

    let purged = storage.purge_run(&RunId::from("run-old")).unwrap();
    assert_eq!(purged, 2);

    assert!(
        storage
            .events_for_run(&RunId::from("run-old"))
            .unwrap()
            .is_empty(),
        "a purged run reads back as unknown, never as corrupt state"
    );
    assert_eq!(
        storage.list_run_ids().unwrap(),
        vec![RunId::from("run-live")],
        "other runs' rows are untouched"
    );
}
