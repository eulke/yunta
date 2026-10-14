//! The SQLite event store: appending, reading back, and the hash chain
//! over what was stored.
//!
//! A seq is assigned per run and never collides, however many threads or
//! connections append at once, and a reopened database keeps what it
//! held. The chain verifies an untouched log intact and names the exact
//! seq where a tampered payload, a deleted row or a single flipped byte
//! breaks it, and a log whose tail was deleted reads as broken.

use std::sync::Arc;
use std::thread;

use proptest::prelude::*;
use yunta_core::events::RunEvent;
use yunta_core::events::{
    EventBody, EventDraft, EventPayload, EventShapeError, PauseReason, RunPausedPayload,
};
use yunta_core::{NodeId, RunId, Seq};
use yunta_storage::{AsyncStorage, ChainVerification, Purge, Storage, StorageError};
use yunta_testkit_core::FixedClock;

#[path = "store/chain.rs"]
mod chain;
#[path = "store/events.rs"]
mod events;

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
        .append(&created_draft("run-1"), &FixedClock)
        .unwrap();
    let seq = storage
        .append(&paused_draft("run-1", "budget exceeded"), &FixedClock)
        .unwrap();
    assert_eq!(seq.get(), 2);

    let events = storage.events_for_run(&run_id).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].seq.get(), 2);
    match events[1].payload() {
        Some(EventPayload::Run(RunEvent::Paused(p))) => assert_eq!(
            p.pause,
            PauseReason::Recorded {
                sentence: "budget exceeded".into()
            }
        ),
        other => panic!("expected RunPaused, got {other:?}"),
    }
}

#[test]
fn seq_is_assigned_monotonically_per_run_and_replay_is_ordered() {
    let (_dir, storage) = open_temp();
    let run_id = RunId::from("run-1");

    storage
        .append(&created_draft("run-1"), &FixedClock)
        .unwrap();
    for i in 0..5 {
        let seq = storage
            .append(&paused_draft("run-1", &format!("reason-{i}")), &FixedClock)
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
        .append(&created_draft("run-a"), &FixedClock)
        .unwrap();
    storage
        .append(&created_draft("run-b"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-a", "a1"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-b", "b1"), &FixedClock)
        .unwrap();
    let seq_a2 = storage
        .append(&paused_draft("run-a", "a2"), &FixedClock)
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
        .append(&created_draft("run-concurrent"), &FixedClock)
        .unwrap();

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let storage = Arc::clone(&storage);
            thread::spawn(move || {
                storage
                    .append(
                        &paused_draft("run-concurrent", &format!("writer-{i}")),
                        &FixedClock,
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
        .append(&created_draft("run-two-conns"), &FixedClock)
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
                            &FixedClock,
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

/// Eight handles open the same new log at once, fifty times over: every
/// opening succeeds, and none reports the database as locked.
#[test]
fn concurrent_opens_of_a_fresh_log_all_succeed() {
    let dir = tempfile::tempdir().unwrap();
    for round in 0..50 {
        let log = dir.path().join(format!("round-{round}.db"));
        let failed = opened_at_once(&log);
        assert!(failed.is_empty(), "round {round}: {failed:?}");
    }
}

/// A log written before the chain had its hash column gains the column
/// once: eight handles opening it at once all succeed.
#[test]
fn concurrent_opens_of_a_log_from_before_the_chain_all_succeed() {
    let dir = tempfile::tempdir().unwrap();
    for round in 0..20 {
        let log = dir.path().join(format!("round-{round}.db"));
        let old = rusqlite::Connection::open(&log).unwrap();
        old.pragma_update(None, "journal_mode", "WAL").unwrap();
        old.execute_batch(
            "CREATE TABLE events (run_id TEXT NOT NULL, seq INTEGER NOT NULL, ts TEXT NOT NULL, \
             node_id TEXT, kind TEXT NOT NULL, payload_json TEXT NOT NULL, \
             schema_version INTEGER NOT NULL, PRIMARY KEY (run_id, seq));",
        )
        .unwrap();
        drop(old);
        let failed = opened_at_once(&log);
        assert!(failed.is_empty(), "round {round}: {failed:?}");
    }
}

/// Opens `log` from eight threads at once: what each opening that failed
/// said.
fn opened_at_once(log: &std::path::Path) -> Vec<String> {
    std::thread::scope(|scope| {
        let opens: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| Storage::open(log).err()))
            .collect();
        opens
            .into_iter()
            .filter_map(|open| open.join().unwrap())
            .map(|e| e.to_string())
            .collect()
    })
}

#[test]
fn list_runs_returns_every_distinct_run() {
    let (_dir, storage) = open_temp();
    storage
        .append(&created_draft("run-a"), &FixedClock)
        .unwrap();
    storage
        .append(&created_draft("run-b"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-a", "a2"), &FixedClock)
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
            .append(&created_draft("run-1"), &FixedClock)
            .unwrap();
        storage
            .append(&paused_draft("run-1", "first session"), &FixedClock)
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
        .append(&created_draft("run-1"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "fine"), &FixedClock)
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

// --- database-level retention ---------------------------------------------

#[test]
fn purge_run_removes_every_row_for_exactly_that_run() {
    let (_dir, storage) = open_temp();
    storage
        .append(&created_draft("run-old"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-old", "done"), &FixedClock)
        .unwrap();
    storage
        .append(&created_draft("run-live"), &FixedClock)
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
        .append(&created_draft("run-reopen"), &FixedClock)
        .unwrap();
    let event = paused_draft("run-reopen", "from the second handle");
    let seq2 = second.append(&event, &FixedClock).unwrap();

    assert_eq!((seq1.get(), seq2.get()), (1, 2));
    let events = storage.events_for_run(&run_id).unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[1].payload(),
        Some(yunta_core::events::EventPayload::Run(RunEvent::Paused(_)))
    ));
}

fn created_draft(run_id: &str) -> EventDraft {
    EventDraft::new(
        RunId::from(run_id),
        None,
        EventPayload::Run(RunEvent::Created(yunta_core::events::RunCreatedPayload {
            checkout: None,
            manifest_hash: yunta_core::sha256_hex(b"abc123manifest"),
            inputs: Default::default(),
            mode: "default".into(),
            promoted_from: None,
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: "deadbeef".into(),
            environment: None,
            left_out: Vec::new(),
            opens_on_base: false,
        })),
    )
    .expect("a draft names its subject")
}

fn paused_draft(run_id: &str, reason: &str) -> EventDraft {
    EventDraft::new(
        RunId::from(run_id),
        None,
        EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
            &PauseReason::Recorded {
                sentence: reason.to_string(),
            },
        ))),
    )
    .expect("a draft names its subject")
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
