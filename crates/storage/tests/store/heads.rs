//! Where every run stands, in one read: each run's last seq, whether the
//! run's head row is there or an older binary wrote the run before there was
//! one.

use super::*;

/// The heads the store reads, as `(run, seq)` pairs a test compares.
fn pairs(heads: Vec<yunta_storage::RunHead>) -> Vec<(String, u64)> {
    heads
        .into_iter()
        .map(|head| (head.run_id.as_str().to_string(), head.seq.get()))
        .collect()
}

/// Every run with an event once, at its last seq. A run with no head row
/// stands for one an older binary wrote, and a purged run is gone.
#[test]
fn each_run_s_head_is_its_last_seq_with_or_without_a_head_row() {
    let (dir, storage) = open_temp();
    for (run, pauses) in [("run-a", 2), ("run-b", 0), ("run-c", 1), ("run-d", 1)] {
        storage.append(&created_draft(run), &FixedClock).unwrap();
        for _ in 0..pauses {
            storage
                .append(&paused_draft(run, "paused"), &FixedClock)
                .unwrap();
        }
    }
    let conn = rusqlite::Connection::open(dir.path().join("yunta.db")).unwrap();
    conn.execute("DELETE FROM run_heads WHERE run_id = 'run-c'", [])
        .unwrap();
    storage.purge_run(&RunId::from("run-d")).unwrap();

    assert_eq!(
        pairs(storage.heads().unwrap()),
        [
            ("run-a".to_string(), 3),
            ("run-b".to_string(), 1),
            ("run-c".to_string(), 2)
        ]
    );
}

/// The async store answers what the store answers.
#[tokio::test]
async fn async_heads_answer_what_the_store_answers() {
    let (dir, storage) = open_temp();
    storage
        .append(&created_draft("run-a"), &FixedClock)
        .unwrap();
    let handle = AsyncStorage::open(dir.path().join("yunta.db"))
        .await
        .unwrap();

    assert_eq!(
        pairs(handle.heads().await.unwrap()),
        pairs(storage.heads().unwrap())
    );
}
