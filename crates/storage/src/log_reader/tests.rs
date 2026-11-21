//! The kept reader on real logs in a temporary directory: its connection
//! refuses a write, `moved` answers true once for each time another
//! connection committed, a first open sets a fresh log up, and what it reads
//! is what the store reads.

use std::path::PathBuf;

use yunta_core::events::{
    EventDraft, EventPayload, PauseReason, RunCreatedPayload, RunEvent, RunPausedPayload,
};
use yunta_core::{RunId, Seq};
use yunta_testkit_core::FixedClock;

use super::LogReader;
use crate::{Storage, StorageError};

/// A directory of the test's own, kept alive by the caller, and where its
/// log goes: nothing is there yet.
fn temp_log() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("yunta.db");
    (dir, log)
}

/// The event that opens a run.
fn created(run: &str) -> EventDraft {
    EventDraft::new(
        RunId::from(run),
        None,
        EventPayload::Run(RunEvent::Created(RunCreatedPayload {
            checkout: None,
            manifest_hash: yunta_core::sha256_hex(b"manifest"),
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
    .unwrap()
}

/// An event a run appends after it opened.
fn paused(run: &str) -> EventDraft {
    EventDraft::new(
        RunId::from(run),
        None,
        EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
            &PauseReason::Recorded {
                sentence: "paused".into(),
            },
        ))),
    )
    .unwrap()
}

#[tokio::test]
async fn its_connection_refuses_an_insert_as_read_only() {
    let (_dir, log) = temp_log();
    let reader = LogReader::open(log.clone()).await.unwrap();

    let error = reader
        .storage
        .append(&created("run-a"), &FixedClock)
        .unwrap_err();

    let StorageError::Append { source, .. } = &error else {
        panic!("an append fails as an append: {error:?}");
    };
    let code = source
        .downcast_ref::<rusqlite::Error>()
        .and_then(rusqlite::Error::sqlite_error_code);
    assert_eq!(code, Some(rusqlite::ErrorCode::ReadOnly), "{error:?}");
    assert!(Storage::open(&log).unwrap().heads().unwrap().is_empty());
}

#[tokio::test]
async fn moved_is_true_once_after_another_connection_commits_at_one_statement_a_look() {
    let (_dir, log) = temp_log();
    let writer = Storage::open(&log).unwrap();
    writer.append(&created("run-a"), &FixedClock).unwrap();
    let reader = LogReader::open(log).await.unwrap();

    assert!(
        !reader.moved().await.unwrap(),
        "a commit before the reader opened is not a move"
    );

    writer.append(&paused("run-a"), &FixedClock).unwrap();
    assert!(reader.moved().await.unwrap(), "another connection appended");
    assert!(
        !reader.moved().await.unwrap(),
        "nothing was written since the last reading"
    );

    writer.append(&paused("run-a"), &FixedClock).unwrap();
    writer.append(&created("run-b"), &FixedClock).unwrap();
    assert!(reader.moved().await.unwrap(), "two commits, one move");
    assert!(!reader.moved().await.unwrap());

    let before = reader.statements();
    assert!(!reader.moved().await.unwrap());
    assert_eq!(
        reader.statements() - before,
        1,
        "a look where nothing moved is one statement"
    );
}

#[tokio::test]
async fn a_reader_opened_where_no_log_is_leaves_one_in_wal_that_the_store_reads() {
    let (_dir, log) = temp_log();
    drop(LogReader::open(log.clone()).await.unwrap());

    let conn = rusqlite::Connection::open(&log).unwrap();
    let mode: String = conn
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    drop(conn);

    let storage = Storage::open(&log).unwrap();
    assert!(storage.heads().unwrap().is_empty());
    storage.append(&created("run-a"), &FixedClock).unwrap();
    assert_eq!(
        storage.events_for_run(&RunId::from("run-a")).unwrap().len(),
        1
    );
}

/// Each run's first event, whatever the length of its log, in one statement: what a
/// follower reads to learn which run a promotion created a run from.
#[tokio::test]
async fn first_events_hand_each_runs_first_event_in_one_statement() {
    let (_dir, log) = temp_log();
    let storage = Storage::open(&log).unwrap();
    for (run, pauses) in [("run-b", 3), ("run-a", 0), ("run-c", 1)] {
        storage.append(&created(run), &FixedClock).unwrap();
        for _ in 0..pauses {
            storage.append(&paused(run), &FixedClock).unwrap();
        }
    }
    let reader = LogReader::open(log).await.unwrap();

    let before = reader.statements();
    let firsts = reader.first_events().await.unwrap();
    assert_eq!(reader.statements() - before, 1, "one statement");
    let opened: Vec<_> = ["run-a", "run-b", "run-c"]
        .into_iter()
        .map(|run| storage.events_for_run(&RunId::from(run)).unwrap()[0].clone())
        .collect();
    assert_eq!(firsts, opened, "each run's first event, by run");
    assert_eq!(storage.first_events().unwrap(), opened);
}

#[tokio::test]
async fn heads_events_and_events_after_are_the_store_s_on_the_same_log() {
    let (_dir, log) = temp_log();
    let storage = Storage::open(&log).unwrap();
    for (run, pauses) in [("run-a", 3), ("run-b", 0), ("run-c", 1)] {
        storage.append(&created(run), &FixedClock).unwrap();
        for _ in 0..pauses {
            storage.append(&paused(run), &FixedClock).unwrap();
        }
    }
    let reader = LogReader::open(log).await.unwrap();

    let heads = reader.heads().await.unwrap();
    assert_eq!(heads.len(), 3);
    assert_eq!(heads, storage.heads().unwrap());

    for run in ["run-a", "run-b", "run-c", "run-none"] {
        let run = RunId::from(run);
        assert_eq!(
            reader.events(&run).await.unwrap(),
            storage.events_for_run(&run).unwrap(),
            "{run}"
        );
        for seq in [Seq::FIRST, Seq::FIRST.next(), Seq::from(4)] {
            assert_eq!(
                reader.events_after(&run, seq).await.unwrap(),
                storage.events_after(&run, seq).unwrap(),
                "{run} after {seq:?}"
            );
        }
    }
    assert_eq!(reader.events(&RunId::from("run-a")).await.unwrap().len(), 4);
    assert_eq!(
        reader
            .events_after(&RunId::from("run-a"), Seq::FIRST.next())
            .await
            .unwrap()
            .len(),
        2
    );
}
