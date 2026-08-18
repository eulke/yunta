use std::sync::Arc;
use std::thread;

use yunta_core::events::{Event, EventPayload, RunPausedPayload};
use yunta_core::RunId;
use yunta_storage::{Storage, StorageError};

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

    let seq = storage
        .append_event(&paused_event("run-1", "budget exceeded"))
        .unwrap();
    assert_eq!(seq, 1);

    let events = storage.events_for_run(&run_id).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].seq, 1);
    match &events[0].payload {
        EventPayload::RunPaused(p) => assert_eq!(p.reason, "budget exceeded"),
        other => panic!("expected RunPaused, got {other:?}"),
    }
}

#[test]
fn seq_is_assigned_monotonically_per_run_and_replay_is_ordered() {
    let (_dir, storage) = open_temp();
    let run_id = RunId::from("run-1");

    for i in 0..5 {
        let seq = storage
            .append_event(&paused_event("run-1", &format!("reason-{i}")))
            .unwrap();
        assert_eq!(seq, i + 1);
    }

    let events = storage.events_for_run(&run_id).unwrap();
    let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![1, 2, 3, 4, 5]);
}

#[test]
fn different_runs_get_independent_sequences() {
    let (_dir, storage) = open_temp();

    storage.append_event(&paused_event("run-a", "a1")).unwrap();
    storage.append_event(&paused_event("run-b", "b1")).unwrap();
    let seq_a2 = storage.append_event(&paused_event("run-a", "a2")).unwrap();

    assert_eq!(seq_a2, 2);
    assert_eq!(
        storage.events_for_run(&RunId::from("run-b")).unwrap().len(),
        1
    );
}

#[test]
fn concurrent_appends_never_lose_or_collide_a_seq() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(Storage::open(&dir.path().join("yunta.db")).unwrap());
    let run_id = RunId::from("run-concurrent");

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
    assert_eq!(assigned_seqs, vec![1, 2, 3, 4, 5, 6, 7, 8]);

    let events = storage.events_for_run(&run_id).unwrap();
    assert_eq!(events.len(), 8);
    let stored_seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
    assert_eq!(stored_seqs, vec![1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn list_run_ids_returns_every_distinct_run() {
    let (_dir, storage) = open_temp();
    storage.append_event(&paused_event("run-a", "a")).unwrap();
    storage.append_event(&paused_event("run-b", "b")).unwrap();
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
        storage
            .append_event(&paused_event("run-1", "first session"))
            .unwrap();
    }

    let storage = Storage::open(&db_path).unwrap();
    let events = storage.events_for_run(&RunId::from("run-1")).unwrap();
    assert_eq!(events.len(), 1);
}

#[test]
fn a_corrupted_payload_surfaces_as_a_typed_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("yunta.db");
    let storage = Storage::open(&db_path).unwrap();
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
