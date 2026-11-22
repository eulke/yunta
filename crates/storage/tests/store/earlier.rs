//! Rows an earlier binary wrote, their timestamps spelled with a `+00:00`
//! offset and a fraction of three, six or nine digits: they verify, read
//! back at their instants and list in their place beside the rows this
//! binary writes.

use super::*;

/// Writes `draft` as the row at `seq` the way an earlier binary wrote it,
/// behind the store's back: its timestamp spelled as `ts`, chained onto
/// `prev_hash` by the store's link (SHA-256 over the previous link, then
/// each field as `len:bytes;` in the table's order), and the run's head
/// moved onto it. Returns the row's link, the next row's `prev_hash`.
fn write_as_an_earlier_binary(
    db: &std::path::Path,
    draft: &EventDraft,
    seq: i64,
    ts: &str,
    prev_hash: &str,
) -> String {
    let payload_json = serde_json::to_string(draft.payload()).unwrap();
    let kind = draft.payload().kind_name();
    let node_id = draft.node_id().map(NodeId::as_str);
    let schema_version = yunta_core::events::KIND_VERSION;
    let mut input = prev_hash.as_bytes().to_vec();
    for field in [
        draft.run_id().as_str(),
        &seq.to_string(),
        ts,
        node_id.unwrap_or(""),
        kind,
        &payload_json,
        &schema_version.to_string(),
    ] {
        input.extend_from_slice(format!("{}:{field};", field.len()).as_bytes());
    }
    let link = yunta_core::sha256_hex(&input).as_str().to_string();
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.execute(
        "INSERT INTO events (run_id, seq, ts, node_id, kind, payload_json, schema_version, \
         event_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            draft.run_id().as_str(),
            seq,
            ts,
            node_id,
            kind,
            payload_json,
            schema_version,
            link
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO run_heads (run_id, seq, event_hash) VALUES (?1, ?2, ?3) \
         ON CONFLICT(run_id) DO UPDATE SET seq = excluded.seq, event_hash = excluded.event_hash",
        rusqlite::params![draft.run_id().as_str(), seq, link],
    )
    .unwrap();
    link
}

/// The link a run made by [`created_draft`] chains its first row onto: its
/// manifest hash, hashed.
fn created_genesis() -> String {
    let manifest_hash = yunta_core::sha256_hex(b"abc123manifest");
    yunta_core::sha256_hex(manifest_hash.as_str().as_bytes())
        .as_str()
        .to_string()
}

/// The run an earlier binary wrote: its `run_created` on a whole second and
/// a node's pause half a second later.
fn written_before(storage: &Storage) -> RunId {
    let run_id = RunId::from("run-1");
    let first = write_as_an_earlier_binary(
        storage.path(),
        &created_draft("run-1"),
        1,
        "2026-09-02T10:00:00+00:00",
        &created_genesis(),
    );
    let paused = paused_draft("run-1", "written before").payload().clone();
    write_as_an_earlier_binary(
        storage.path(),
        &EventDraft::new(run_id.clone(), Some(NodeId::from("plan")), paused).unwrap(),
        2,
        "2026-09-02T10:00:00.500+00:00",
        &first,
    );
    run_id
}

#[test]
fn a_chain_an_earlier_binary_wrote_verifies_and_reads_at_its_instants() {
    let (_dir, storage) = open_temp();
    let run_id = written_before(&storage);
    assert_eq!(
        storage.verify_chain(&run_id).unwrap(),
        ChainVerification::Intact { events: 2 }
    );
    let read: Vec<_> = storage
        .events_for_run(&run_id)
        .unwrap()
        .into_iter()
        .map(|event| event.timestamp)
        .collect();
    let written = ["2026-09-02T10:00:00Z", "2026-09-02T10:00:00.5Z"]
        .map(|instant| yunta_testkit_core::AtClock::rfc3339(instant).0);
    assert_eq!(read, written);
}

#[test]
fn an_event_appended_onto_a_chain_an_earlier_binary_wrote_keeps_it_intact() {
    let (_dir, storage) = open_temp();
    let run_id = written_before(&storage);
    let now = yunta_testkit_core::AtClock::rfc3339("2026-09-02T10:00:01.25Z");
    storage
        .append(&paused_draft("run-1", "written now"), &now)
        .unwrap();
    assert_eq!(
        storage.verify_chain(&run_id).unwrap(),
        ChainVerification::Intact { events: 3 }
    );
    let last = storage.events_for_run(&run_id).unwrap().pop().unwrap();
    assert_eq!(last.timestamp, now.0);
}

/// Runs whose first events fall in one second, at every width a fraction
/// is written in (none, tenths, milliseconds, microseconds, nanoseconds),
/// appended out of order beside a run an earlier binary wrote with a
/// `+00:00` offset, list by instant: the order never follows how the
/// stored text spells the fraction.
#[test]
fn runs_in_one_second_list_by_instant_whatever_their_fraction() {
    let (_dir, storage) = open_temp();
    let at = yunta_testkit_core::AtClock::rfc3339;
    for (run_id, instant) in [
        ("run-tenths", "2027-01-15T08:00:00.5Z"),
        ("run-nanos", "2027-01-15T08:00:00.123456789Z"),
        ("run-whole", "2027-01-15T08:00:00Z"),
        ("run-micros", "2027-01-15T08:00:00.123456Z"),
        ("run-millis", "2027-01-15T08:00:00.123Z"),
    ] {
        storage
            .append(&created_draft(run_id), &at(instant))
            .unwrap();
    }
    write_as_an_earlier_binary(
        storage.path(),
        &created_draft("run-earlier-binary"),
        1,
        "2027-01-15T08:00:00.250+00:00",
        &created_genesis(),
    );

    let listed: Vec<_> = storage
        .list_runs()
        .unwrap()
        .into_iter()
        .map(|run| (run.run_id.as_str().to_string(), run.created_at))
        .collect();
    let in_instant_order: Vec<_> = [
        ("run-whole", "2027-01-15T08:00:00Z"),
        ("run-millis", "2027-01-15T08:00:00.123Z"),
        ("run-micros", "2027-01-15T08:00:00.123456Z"),
        ("run-nanos", "2027-01-15T08:00:00.123456789Z"),
        ("run-earlier-binary", "2027-01-15T08:00:00.25Z"),
        ("run-tenths", "2027-01-15T08:00:00.5Z"),
    ]
    .into_iter()
    .map(|(run_id, instant)| (run_id.to_string(), at(instant).0))
    .collect();
    assert_eq!(listed, in_instant_order);
}
