use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};
use yunta_core::events::{Event, EventPayload};
use yunta_core::{NodeId, RunId};

use crate::error::{Result, StorageError};

/// The schema migration, embedded in the binary rather than shipped as a
/// file — the whole schema is one append-only table, so there is nothing
/// a migration runner would buy over `CREATE TABLE IF NOT EXISTS` today.
/// `event_hash` is nullable at the SQL level only so the `ALTER TABLE`
/// migration below can add it to databases created before the hash chain
/// existed — every append since writes it, and `verify_chain` reports a
/// NULL as a break, never skips it.
const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS events (
    run_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    ts TEXT NOT NULL,
    node_id TEXT,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    event_hash TEXT,
    PRIMARY KEY (run_id, seq)
);
";

/// `yunta verify` / receipt-time integrity: the chain covers integrity
/// and order, never authenticity.
#[derive(Debug, Clone, PartialEq)]
pub enum ChainVerification {
    Intact {
        events: usize,
    },
    /// The first event where the chain stops holding, with what exactly
    /// stopped holding.
    Broken {
        seq: u64,
        detail: String,
    },
}

/// One persisted row as `verify_chain` reads it back: `(seq, ts,
/// node_id, kind, payload_json, schema_version, event_hash)`.
type StoredRow = (
    i64,
    String,
    Option<String>,
    String,
    String,
    u32,
    Option<String>,
);

/// One event's chain hash: SHA-256 over the
/// previous hash followed by the structural fields in the schema's own
/// fixed order, each length-prefixed (`len:bytes;`) so no field boundary
/// is ambiguous — computed over the bytes exactly as persisted, before
/// any read-time normalization. An absent `node_id` hashes as the empty
/// field; an empty node id is not constructible from any workflow.
/// The structural fields hashed into one chain link, in the schema's own
/// fixed order — see [`chain_hash`].
struct ChainHashFields<'a> {
    prev_hash: &'a str,
    run_id: &'a str,
    seq: i64,
    ts: &'a str,
    node_id: Option<&'a str>,
    kind: &'a str,
    payload_json: &'a str,
    schema_version: u32,
}

fn chain_hash(fields: ChainHashFields) -> String {
    let mut input = Vec::new();
    input.extend_from_slice(fields.prev_hash.as_bytes());
    for field in [
        fields.run_id,
        &fields.seq.to_string(),
        fields.ts,
        fields.node_id.unwrap_or(""),
        fields.kind,
        fields.payload_json,
        &fields.schema_version.to_string(),
    ] {
        input.extend_from_slice(field.len().to_string().as_bytes());
        input.push(b':');
        input.extend_from_slice(field.as_bytes());
        input.push(b';');
    }
    yunta_core::sha256_hex(&input)
}

/// Genesis: `H0 = SHA-256(manifest_hash)` — deterministic, unique per
/// run, no arbitrary constant.
fn genesis_hash(manifest_hash: &str) -> String {
    yunta_core::sha256_hex(manifest_hash.as_bytes())
}

/// The event log. SQLite is the only backend, and nothing about it leaks
/// past this interface: every method takes and returns `yunta_core`
/// types, never rusqlite's.
///
/// A single mutex-guarded connection serializes writes at the Rust level;
/// WAL mode (set in [`Storage::open`]) is what lets readers proceed
/// without blocking on that writer.
pub struct Storage {
    conn: Mutex<Connection>,
    /// Where this handle was opened — what [`Storage::reopen`] uses to
    /// mint an independent connection onto the same database.
    path: std::path::PathBuf,
}

fn lock(conn: &Mutex<Connection>) -> std::sync::MutexGuard<'_, Connection> {
    // A poisoned lock here means some other thread panicked while
    // holding it — the connection itself is still whatever it was, so
    // recovering the guard is safer than making every caller of this
    // crate handle a panic that isn't theirs (and CLAUDE.md rules out
    // `.lock().unwrap()` outside tests).
    conn.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Storage {
    /// Opens (creating if needed) the event log at `path` in WAL mode,
    /// running the embedded schema migration.
    pub fn open(path: &Path) -> Result<Self> {
        let open_err = |source| StorageError::Open {
            path: path.to_path_buf(),
            source,
        };

        let conn = Connection::open(path).map_err(open_err)?;
        // WAL admits exactly one writer at a time; a second handle onto
        // the same database (a status/follower reader, a per-session
        // run-tools listener) must wait for a busy writer instead of
        // surfacing SQLITE_BUSY as a spurious append failure. Writes
        // here are all sub-millisecond appends — 5s of patience means
        // something is truly wedged, not busy. Set before the journal
        // pragma so even that first statement waits rather than fails
        // when another connection happens to be mid-write.
        // Note the timeout alone is NOT enough for `append_event` — see
        // the BEGIN IMMEDIATE note there.
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(open_err)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(open_err)?;
        conn.execute_batch(SCHEMA_V1).map_err(open_err)?;

        // A database created before the chain existed has the table but
        // not the column — added here once, with old rows left NULL
        // (visible to `verify_chain` as a break, never silently
        // backfilled with hashes nothing ever wrote).
        let has_hash_column = conn
            .prepare("SELECT 1 FROM pragma_table_info('events') WHERE name = 'event_hash'")
            .and_then(|mut stmt| stmt.exists([]))
            .map_err(open_err)?;
        if !has_hash_column {
            conn.execute("ALTER TABLE events ADD COLUMN event_hash TEXT", [])
                .map_err(open_err)?;
        }

        Ok(Self {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
        })
    }

    /// A second, independent handle onto the same database — what a
    /// concurrent reader/writer with a `'static` life of its own (the
    /// per-session run-tools listener; the CLI's `--follow` poller
    /// already does this by path from outside) opens instead of
    /// sharing this handle's connection. WAL + the busy timeout above
    /// are what make the concurrency safe; `seq` assignment stays
    /// correct because [`Storage::append_event`] computes it inside its
    /// own transaction.
    pub fn reopen(&self) -> Result<Self> {
        Self::open(&self.path)
    }

    /// Appends one event, assigning the next `seq` for its `run_id`
    /// (append-only — nothing here ever updates or deletes a row).
    /// Returns the assigned `seq`.
    pub fn append_event(&self, event: &Event) -> Result<u64> {
        let append_err = |source| StorageError::Append {
            run_id: event.run_id.clone(),
            source,
        };

        let mut conn = lock(&self.conn);
        // BEGIN IMMEDIATE, not deferred: this transaction reads
        // (`MAX(seq)`, the previous hash) before it writes, and a
        // deferred read→write upgrade against a concurrently-busy writer
        // (another connection — the per-session run-tools listener)
        // returns SQLITE_BUSY *immediately*: SQLite refuses to invoke
        // the busy handler on an upgrade, since waiting there can
        // deadlock, so the connection's `busy_timeout` never applies.
        // Taking the write lock up front puts the wait where the busy
        // handler does work, and holds the lock across the read+insert —
        // which is also what keeps `seq` correct across connections.
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(append_err)?;

        let seq: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE run_id = ?1",
                params![event.run_id.as_str()],
                |row| row.get(0),
            )
            .map_err(append_err)?;

        let payload_json =
            serde_json::to_string(&event.payload).map_err(|source| StorageError::Serialize {
                run_id: event.run_id.clone(),
                source,
            })?;

        // The chain hash, computed inside the same transaction that
        // assigns `seq` — the previous event's stored hash (or the
        // manifest-derived genesis for the run's first event) anchors it.
        let prev_hash = if seq == 1 {
            let EventPayload::RunCreated(created) = &event.payload else {
                return Err(StorageError::GenesisMissing {
                    run_id: event.run_id.clone(),
                });
            };
            genesis_hash(&created.manifest_hash)
        } else {
            tx.query_row(
                "SELECT event_hash FROM events WHERE run_id = ?1 AND seq = ?2",
                params![event.run_id.as_str(), seq - 1],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(append_err)?
            // A NULL here means the run predates the hash chain — its
            // chain is already unverifiable; anchoring on the empty
            // string keeps the append working while `verify_chain`
            // reports the truth.
            .unwrap_or_default()
        };

        let ts = event.timestamp.to_rfc3339();
        let event_hash = chain_hash(ChainHashFields {
            prev_hash: &prev_hash,
            run_id: event.run_id.as_str(),
            seq,
            ts: &ts,
            node_id: event.node_id.as_ref().map(NodeId::as_str),
            kind: event.payload.kind_name(),
            payload_json: &payload_json,
            schema_version: event.payload.schema_version(),
        });

        tx.execute(
            "INSERT INTO events (run_id, seq, ts, node_id, kind, payload_json, schema_version, \
             event_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.run_id.as_str(),
                seq,
                ts,
                event.node_id.as_ref().map(NodeId::as_str),
                event.payload.kind_name(),
                payload_json,
                event.payload.schema_version(),
                event_hash,
            ],
        )
        .map_err(append_err)?;

        tx.commit().map_err(append_err)?;

        Ok(seq as u64)
    }

    /// Walks the run's whole chain recomputing every hash from the bytes
    /// as persisted: an altered payload, a deleted, inserted or reordered
    /// event, an altered stored hash, or a missing one all surface as
    /// [`ChainVerification::Broken`] naming the exact seq.
    /// Runs automatically at receipt time and on demand via
    /// `yunta verify <run_id>`.
    pub fn verify_chain(&self, run_id: &RunId) -> Result<ChainVerification> {
        let read_err = |source| StorageError::Read {
            run_id: run_id.clone(),
            source,
        };

        let conn = lock(&self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT seq, ts, node_id, kind, payload_json, schema_version, event_hash
                 FROM events WHERE run_id = ?1 ORDER BY seq ASC",
            )
            .map_err(read_err)?;
        let rows: Vec<StoredRow> = stmt
            .query_map(params![run_id.as_str()], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .map_err(read_err)?
            .collect::<std::result::Result<_, _>>()
            .map_err(read_err)?;

        if rows.is_empty() {
            return Err(StorageError::VerifyUnknownRun {
                run_id: run_id.clone(),
            });
        }

        let mut prev_hash: Option<String> = None;
        for (index, (seq, ts, node_id, kind, payload_json, schema_version, stored_hash)) in
            rows.iter().enumerate()
        {
            let expected_seq = index as i64 + 1;
            if *seq != expected_seq {
                return Ok(ChainVerification::Broken {
                    seq: *seq as u64,
                    detail: format!(
                        "expected seq {expected_seq} but found {seq} — an event was deleted, \
                         inserted or reordered"
                    ),
                });
            }
            let Some(stored_hash) = stored_hash else {
                return Ok(ChainVerification::Broken {
                    seq: *seq as u64,
                    detail: "event has no stored hash (written before the chain existed)"
                        .to_string(),
                });
            };
            let anchor = match &prev_hash {
                Some(prev) => prev.clone(),
                None => {
                    // Genesis: the first event must be `run_created` and
                    // its own payload carries the manifest hash.
                    if kind != "run_created" {
                        return Ok(ChainVerification::Broken {
                            seq: *seq as u64,
                            detail: format!(
                                "first event is `{kind}`, not `run_created` — no genesis"
                            ),
                        });
                    }
                    let manifest_hash = serde_json::from_str::<serde_json::Value>(payload_json)
                        .ok()
                        .and_then(|v| {
                            v.get("manifest_hash")
                                .and_then(|h| h.as_str().map(String::from))
                        });
                    let Some(manifest_hash) = manifest_hash else {
                        return Ok(ChainVerification::Broken {
                            seq: *seq as u64,
                            detail:
                                "run_created payload has no readable manifest_hash — no genesis"
                                    .to_string(),
                        });
                    };
                    genesis_hash(&manifest_hash)
                }
            };
            let recomputed = chain_hash(ChainHashFields {
                prev_hash: &anchor,
                run_id: run_id.as_str(),
                seq: *seq,
                ts,
                node_id: node_id.as_deref(),
                kind,
                payload_json,
                schema_version: *schema_version,
            });
            if &recomputed != stored_hash {
                return Ok(ChainVerification::Broken {
                    seq: *seq as u64,
                    detail: "stored hash does not match the recomputed chain — the event or \
                             its predecessor's hash was altered"
                        .to_string(),
                });
            }
            prev_hash = Some(stored_hash.clone());
        }

        Ok(ChainVerification::Intact { events: rows.len() })
    }

    /// All events for a run, ordered by `seq` ascending — the order
    /// replay depends on.
    pub fn events_for_run(&self, run_id: &RunId) -> Result<Vec<Event>> {
        let read_err = |source| StorageError::Read {
            run_id: run_id.clone(),
            source,
        };

        let conn = lock(&self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT seq, ts, node_id, payload_json
                 FROM events WHERE run_id = ?1 ORDER BY seq ASC",
            )
            .map_err(read_err)?;

        let rows = stmt
            .query_map(params![run_id.as_str()], |row| {
                let seq: i64 = row.get(0)?;
                let ts: String = row.get(1)?;
                let node_id: Option<String> = row.get(2)?;
                let payload_json: String = row.get(3)?;
                Ok((seq as u64, ts, node_id, payload_json))
            })
            .map_err(read_err)?;

        let mut events = Vec::new();
        for row in rows {
            let (seq, ts, node_id, payload_json) = row.map_err(read_err)?;

            let payload: EventPayload = serde_json::from_str(&payload_json).map_err(|source| {
                StorageError::CorruptPayload {
                    run_id: run_id.clone(),
                    seq,
                    source,
                }
            })?;

            let timestamp = chrono::DateTime::parse_from_rfc3339(&ts)
                .map_err(|source| StorageError::CorruptTimestamp {
                    run_id: run_id.clone(),
                    seq,
                    source,
                })?
                .with_timezone(&chrono::Utc);

            let node_id = node_id
                .map(NodeId::try_from)
                .transpose()
                .map_err(|source| StorageError::CorruptNodeId {
                    run_id: run_id.clone(),
                    seq,
                    source,
                })?;

            events.push(Event {
                run_id: run_id.clone(),
                seq,
                timestamp,
                node_id,
                payload,
            });
        }
        Ok(events)
    }

    /// Deletes every row for `run_id` — the database side of
    /// `storage.retention_days`. The one deliberate exception to the
    /// append-only discipline, and the *caller* (`yunta gc`) owns the
    /// safety rule: rows die only after the run.dir (whose exported
    /// `events.jsonl` is the self-contained copy) is already gone — the
    /// database is never the first copy to die. Returns how many rows
    /// were removed.
    pub fn purge_run(&self, run_id: &RunId) -> Result<usize> {
        let conn = lock(&self.conn);
        let removed = conn
            .execute(
                "DELETE FROM events WHERE run_id = ?1",
                params![run_id.as_str()],
            )
            .map_err(|source| StorageError::Read {
                run_id: run_id.clone(),
                source,
            })?;
        Ok(removed)
    }

    /// Every distinct `run_id` with at least one event, for `yunta run
    /// --runs` to enumerate.
    pub fn list_run_ids(&self) -> Result<Vec<RunId>> {
        let conn = lock(&self.conn);
        let mut stmt = conn
            .prepare("SELECT DISTINCT run_id FROM events ORDER BY run_id ASC")
            .map_err(|source| StorageError::ListRuns { source })?;

        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|source| StorageError::ListRuns { source })?;

        let mut run_ids = Vec::new();
        for row in rows {
            let run_id = row.map_err(|source| StorageError::ListRuns { source })?;
            run_ids.push(
                RunId::try_from(run_id).map_err(|source| StorageError::CorruptRunId { source })?,
            );
        }
        Ok(run_ids)
    }
}
