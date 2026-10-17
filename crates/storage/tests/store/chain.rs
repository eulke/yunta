//! The event hash chain over what the store holds: an untouched log
//! verifies intact, and each way a log can be altered breaks it at the seq
//! where the alteration shows.

use super::*;

/// A draft of `payload` in `run_id`, scoped to a node when `node_scoped`.
fn scoped_draft(run_id: &RunId, node_scoped: bool, payload: EventPayload) -> EventDraft {
    let node_id = node_scoped.then(|| NodeId::from("plan"));
    EventDraft::new(run_id.clone(), node_id, payload).expect("a draft names its subject")
}

#[test]
fn an_untouched_log_verifies_intact() {
    let (_dir, storage) = open_temp();
    let run_id = RunId::from("run-1");
    storage
        .append(&created_draft("run-1"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "first pause"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "second pause"), &FixedClock)
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
        .append(&created_draft("run-1"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "honest"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "later"), &FixedClock)
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
            assert_eq!(
                detail,
                "stored hash does not match the recomputed chain — the event or its \
                 predecessor's hash was altered"
            );
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
        .append(&created_draft("run-1"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "a"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "b"), &FixedClock)
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
            assert_eq!(
                detail,
                "expected seq 2 but found 3 — an event was deleted, inserted or reordered"
            );
        }
        other => panic!("expected a broken chain, got {other:?}"),
    }
}

#[test]
fn a_row_without_a_stored_hash_reads_as_broken_as_written_before_the_chain() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("yunta.db");
    let storage = Storage::open(&db).unwrap();
    let run_id = RunId::from("run-1");
    storage
        .append(&created_draft("run-1"), &FixedClock)
        .unwrap();
    storage
        .append(&paused_draft("run-1", "a"), &FixedClock)
        .unwrap();
    drop(storage);

    // A row the chain never hashed, as a log written without the column holds.
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE events SET event_hash = NULL WHERE run_id = 'run-1' AND seq = 2",
        [],
    )
    .unwrap();
    drop(conn);

    let storage = Storage::open(&db).unwrap();
    match storage.verify_chain(&run_id).unwrap() {
        ChainVerification::Broken { seq, detail } => {
            assert_eq!(seq.get(), 2);
            assert_eq!(
                detail,
                "event has no stored hash (written before the chain existed)"
            );
        }
        other => panic!("expected a broken chain at seq 2, got {other:?}"),
    }
}

#[test]
fn a_deleted_tail_breaks_the_chain_at_the_stored_head() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("yunta.db");
    let storage = Storage::open(&db).unwrap();
    for draft in [
        created_draft("run-1"),
        paused_draft("run-1", "a"),
        paused_draft("run-1", "b"),
    ] {
        storage.append(&draft, &FixedClock).unwrap();
    }
    drop(storage);
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute("DELETE FROM events WHERE run_id = 'run-1' AND seq = 3", [])
        .unwrap();
    drop(conn);
    let storage = Storage::open(&db).unwrap();
    assert!(
        matches!(
            storage.verify_chain(&RunId::from("run-1")).unwrap(),
            ChainVerification::Broken { .. }
        ),
        "a log that lost its tail reads as whole"
    );
}

#[test]
fn a_log_without_a_stored_head_reads_as_broken() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("yunta.db");
    let storage = Storage::open(&db).unwrap();
    for draft in [created_draft("run-1"), paused_draft("run-1", "a")] {
        storage.append(&draft, &FixedClock).unwrap();
    }
    drop(storage);
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute("DELETE FROM run_heads WHERE run_id = 'run-1'", [])
        .unwrap();
    drop(conn);
    let storage = Storage::open(&db).unwrap();
    match storage.verify_chain(&RunId::from("run-1")).unwrap() {
        ChainVerification::Broken { detail, .. } => {
            assert!(detail.contains("no stored head"), "{detail}");
        }
        intact => panic!("a log with no stored head reads as {intact:?}"),
    }
}

#[test]
fn a_stored_head_that_names_another_hash_reads_as_broken() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("yunta.db");
    let storage = Storage::open(&db).unwrap();
    for draft in [created_draft("run-1"), paused_draft("run-1", "a")] {
        storage.append(&draft, &FixedClock).unwrap();
    }
    drop(storage);
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE run_heads SET event_hash = 'not-the-last-hash' WHERE run_id = 'run-1'",
        [],
    )
    .unwrap();
    drop(conn);
    let storage = Storage::open(&db).unwrap();
    match storage.verify_chain(&RunId::from("run-1")).unwrap() {
        ChainVerification::Broken { detail, .. } => {
            assert!(detail.contains("does not name the last event"), "{detail}");
        }
        intact => panic!("a head naming another hash reads as {intact:?}"),
    }
}

#[test]
fn a_single_byte_flip_at_any_position_breaks_the_chain() {
    // Every position — the run_created genesis and each event after it —
    // corrupted in its own fresh store, since verify_chain stops at the first
    // break it finds and one corruption per run isolates each position.
    let drafts = [
        created_draft("run-1"),
        paused_draft("run-1", "one"),
        paused_draft("run-1", "two"),
        paused_draft("run-1", "three"),
    ];
    for seq in 1..=drafts.len() as i64 {
        let (_dir, storage) = open_temp();
        for draft in &drafts {
            storage.append(draft, &FixedClock).unwrap();
        }

        storage
            .corrupt_event_payload(&RunId::from("run-1"), Seq::try_from(seq).unwrap())
            .unwrap();

        match storage.verify_chain(&RunId::from("run-1")).unwrap() {
            ChainVerification::Broken { .. } => {}
            ChainVerification::Intact { .. } => {
                panic!("a corrupted event at seq {seq} went undetected")
            }
        }
    }
}

/// One persisted column of one stored event, as the chain hashes it.
struct PersistedCell {
    seq: i64,
    column: &'static str,
    text: String,
}

/// Every text the hash chain covers, event by event and column by column,
/// read straight off disk: the timestamp, the node id where a row carries
/// one, the kind, the payload, the schema version, and the stored hash the
/// walk compares its recomputation against.
///
/// `run_id` and `seq` stay out: altering either moves an event to another
/// run or on top of its neighbour, which is the gap a deleted or reordered
/// event leaves rather than an altered event's own bytes.
fn persisted_cells(db: &std::path::Path, run_id: &str) -> Vec<PersistedCell> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT seq, ts, node_id, kind, payload_json, schema_version, event_hash
             FROM events WHERE run_id = ?1 ORDER BY seq ASC",
        )
        .unwrap();
    stmt.query_map(rusqlite::params![run_id], |row| {
        let seq: i64 = row.get(0)?;
        let columns = [
            ("ts", Some(row.get::<_, String>(1)?)),
            ("node_id", row.get::<_, Option<String>>(2)?),
            ("kind", Some(row.get::<_, String>(3)?)),
            ("payload_json", Some(row.get::<_, String>(4)?)),
            ("schema_version", Some(row.get::<_, u32>(5)?.to_string())),
            ("event_hash", Some(row.get::<_, String>(6)?)),
        ];
        Ok(columns
            .into_iter()
            .filter_map(|(column, text)| text.map(|text| PersistedCell { seq, column, text }))
            .collect::<Vec<_>>())
    })
    .unwrap()
    .flat_map(|row| row.unwrap())
    .collect()
}

/// The offsets of `text` whose byte a single-bit flip keeps printable
/// ASCII: bit 0 toggles inside a pair of adjacent codes, so `0x20..0x7f`
/// maps onto itself. The altered column is still valid UTF-8, a digit is
/// still a digit, and a column SQLite stores as an integer still reads
/// back as one — the only difference the store sees is the one byte.
fn flippable_offsets(text: &str) -> Vec<usize> {
    text.bytes()
        .enumerate()
        .filter(|(_, byte)| (0x20..0x7f).contains(byte))
        .map(|(offset, _)| offset)
        .collect()
}

/// Flips bit 0 of the byte at `offset` of one persisted column, behind the
/// store's back — a disk that changed under a log whose own interface
/// never rewrites a row. The column name is one of this file's own, never
/// generated input.
fn flip_one_byte(db: &std::path::Path, run_id: &str, cell: &PersistedCell, offset: usize) {
    let mut bytes = cell.text.clone().into_bytes();
    bytes[offset] ^= 1;
    let flipped = String::from_utf8(bytes).unwrap();
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.execute(
        &format!(
            "UPDATE events SET {} = ?1 WHERE run_id = ?2 AND seq = ?3",
            cell.column
        ),
        rusqlite::params![flipped, run_id, cell.seq],
    )
    .unwrap();
}

proptest! {
    /// The chain covers every append and every byte of one: a log grown by
    /// any sequence of payloads, node-scoped or not, verifies intact over
    /// exactly the events it was given; and flipping a single bit of any
    /// byte the chain hashes — a timestamp, a node id, a kind, a payload, a
    /// schema version, or an event's own stored hash — breaks verification
    /// at exactly the event that changed.
    #[test]
    fn any_append_sequence_verifies_and_any_flipped_byte_is_caught(
        appends in prop::collection::vec(
            (0..yunta_testkit_core::all_kinds().len(), any::<bool>()),
            0..6,
        ),
        which_cell in any::<prop::sample::Index>(),
        which_byte in any::<prop::sample::Index>(),
    ) {
        let kinds = yunta_testkit_core::all_kinds();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("yunta.db");
        let storage = Storage::open(&db).unwrap();
        let run_id = RunId::from("run-prop");

        // A run's first event is its `run_created`: the chain's genesis.
        storage
            .append(&created_draft(run_id.as_str()), &FixedClock)
            .unwrap();
        for (kind, node_scoped) in &appends {
            storage
                .append(
                    &scoped_draft(&run_id, *node_scoped, kinds[*kind].clone()),
                    &FixedClock,
                )
                .unwrap();
        }

        prop_assert_eq!(
            storage.verify_chain(&run_id).unwrap(),
            ChainVerification::Intact { events: appends.len() + 1 }
        );

        let cells = persisted_cells(&db, run_id.as_str());
        let cell = &cells[which_cell.index(cells.len())];
        let offsets = flippable_offsets(&cell.text);
        let offset = offsets[which_byte.index(offsets.len())];
        flip_one_byte(&db, run_id.as_str(), cell, offset);

        match storage.verify_chain(&run_id).unwrap() {
            ChainVerification::Broken { seq, detail } => {
                prop_assert_eq!(u64::try_from(cell.seq).unwrap(), seq.get(), "{}", detail);
            }
            ChainVerification::Intact { .. } => panic!(
                "byte {offset} of `{}` at seq {} is flipped and the chain still verifies intact",
                cell.column, cell.seq
            ),
        }
    }
}

#[test]
fn a_run_whose_first_event_is_not_run_created_is_refused_at_append() {
    let (_dir, storage) = open_temp();
    let err = storage
        .append(
            &paused_draft("run-genesis", "no birth certificate"),
            &FixedClock,
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
