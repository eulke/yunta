use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{params, Connection};
use yunta_core::events::{EventBody, EventDraft, EventPayload, EventShapeError, StoredEvent};
use yunta_core::{Clock, NodeId, RunId, Seq};

use crate::error::{Cause, Result, StorageError};

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
        seq: Seq,
        detail: String,
    },
}

/// One run as `list_runs` reports it: its id and when its first event
/// was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunListing {
    pub run_id: RunId,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// What `purge_run` removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Purge {
    pub rows: usize,
}

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

/// One event's chain hash: SHA-256 over the previous hash followed by
/// the structural fields in the schema's own fixed order, each
/// length-prefixed (`len:bytes;`) so no field boundary is ambiguous —
/// computed over the bytes exactly as persisted, before any read-time
/// normalization. An absent `node_id` hashes as the empty field; an
/// empty node id is not constructible from any workflow.
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

fn cause(error: rusqlite::Error) -> Cause {
    Box::new(error)
}

/// The event log. SQLite is the only backend, and nothing about it leaks
/// past this interface: every method takes and returns `yunta_core`
/// types, and a failure's cause is kept behind a type-erased error.
///
/// A single mutex-guarded connection serializes writes at the Rust level;
/// WAL mode (set in [`Storage::open`]) is what lets readers proceed
/// without blocking on that writer.
pub struct Storage {
    conn: Mutex<Connection>,
    /// Where this handle was opened — what [`Storage::async_handle`]
    /// hands to async code, which opens its own connections there.
    path: PathBuf,
}

impl std::fmt::Debug for Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Storage").field("path", &self.path).finish()
    }
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
        let open_err = |error| StorageError::Open {
            path: path.to_path_buf(),
            source: cause(error),
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
        // Note the timeout alone is NOT enough for `append` — see the
        // BEGIN IMMEDIATE note there.
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

    /// Where this log lives.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The handle async code uses on this same log — every call it makes
    /// opens its own connection on a blocking thread.
    pub fn async_handle(&self) -> crate::AsyncStorage {
        crate::AsyncStorage::at(self.path.clone())
    }

    /// Appends one draft, assigning the next `seq` for its run and the
    /// timestamp `clock` reports (append-only — nothing here ever
    /// updates or deletes a row). Returns the assigned `seq`.
    pub fn append(&self, draft: &EventDraft, clock: &dyn Clock) -> Result<Seq> {
        self.append_at(draft, clock.now())
    }

    /// [`Storage::append`] with the timestamp already read from the
    /// clock — what an async caller does before hopping to a blocking
    /// thread, so the instant recorded is the one its clock reported.
    pub fn append_at(&self, draft: &EventDraft, at: chrono::DateTime<chrono::Utc>) -> Result<Seq> {
        let append_err = |error| StorageError::Append {
            run_id: draft.run_id.clone(),
            source: cause(error),
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
                params![draft.run_id.as_str()],
                |row| row.get(0),
            )
            .map_err(append_err)?;

        let payload_json =
            serde_json::to_string(&draft.payload).map_err(|source| StorageError::Serialize {
                run_id: draft.run_id.clone(),
                source,
            })?;

        // The chain hash, computed inside the same transaction that
        // assigns `seq` — the previous event's stored hash (or the
        // manifest-derived genesis for the run's first event) anchors it.
        let prev_hash = if seq == 1 {
            let EventPayload::RunCreated(created) = &draft.payload else {
                return Err(StorageError::GenesisMissing {
                    run_id: draft.run_id.clone(),
                });
            };
            genesis_hash(&created.manifest_hash)
        } else {
            tx.query_row(
                "SELECT event_hash FROM events WHERE run_id = ?1 AND seq = ?2",
                params![draft.run_id.as_str(), seq - 1],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(append_err)?
            // A NULL here means the run predates the hash chain — its
            // chain is already unverifiable; anchoring on the empty
            // string keeps the append working while `verify_chain`
            // reports the truth.
            .unwrap_or_default()
        };

        let ts = at.to_rfc3339();
        let event_hash = chain_hash(ChainHashFields {
            prev_hash: &prev_hash,
            run_id: draft.run_id.as_str(),
            seq,
            ts: &ts,
            node_id: draft.node_id.as_ref().map(NodeId::as_str),
            kind: draft.payload.kind_name(),
            payload_json: &payload_json,
            schema_version: draft.payload.schema_version(),
        });

        tx.execute(
            "INSERT INTO events (run_id, seq, ts, node_id, kind, payload_json, schema_version, \
             event_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                draft.run_id.as_str(),
                seq,
                ts,
                draft.node_id.as_ref().map(NodeId::as_str),
                draft.payload.kind_name(),
                payload_json,
                draft.payload.schema_version(),
                event_hash,
            ],
        )
        .map_err(append_err)?;

        tx.commit().map_err(append_err)?;

        Seq::try_from(seq).map_err(|source| StorageError::CorruptSeq {
            run_id: draft.run_id.clone(),
            source,
        })
    }

    /// Walks the run's whole chain recomputing every hash from the bytes
    /// as persisted, one row at a time: an altered payload, a deleted,
    /// inserted or reordered event, an altered stored hash, or a missing
    /// one all surface as [`ChainVerification::Broken`] naming the exact
    /// seq. Runs automatically at receipt time and on demand via
    /// `yunta verify <run_id>`.
    pub fn verify_chain(&self, run_id: &RunId) -> Result<ChainVerification> {
        let read_err = |error| StorageError::Read {
            run_id: run_id.clone(),
            source: cause(error),
        };

        let conn = lock(&self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT seq, ts, node_id, kind, payload_json, schema_version, event_hash
                 FROM events WHERE run_id = ?1 ORDER BY seq ASC",
            )
            .map_err(read_err)?;
        let rows = stmt
            .query_map(params![run_id.as_str()], |row| {
                Ok(StoredRow {
                    seq: row.get(0)?,
                    ts: row.get(1)?,
                    node_id: row.get(2)?,
                    kind: row.get(3)?,
                    payload_json: row.get(4)?,
                    schema_version: row.get(5)?,
                    event_hash: row.get(6)?,
                })
            })
            .map_err(read_err)?;

        let mut prev_hash: Option<String> = None;
        let mut verified = 0usize;
        for row in rows {
            let row = row.map_err(read_err)?;
            let expected_seq = verified as i64 + 1;
            let seq = Seq::try_from(row.seq).map_err(|source| StorageError::CorruptSeq {
                run_id: run_id.clone(),
                source,
            })?;
            if row.seq != expected_seq {
                return Ok(ChainVerification::Broken {
                    seq,
                    detail: format!(
                        "expected seq {expected_seq} but found {} — an event was deleted, \
                         inserted or reordered",
                        row.seq
                    ),
                });
            }
            let Some(stored_hash) = &row.event_hash else {
                return Ok(ChainVerification::Broken {
                    seq,
                    detail: "event has no stored hash (written before the chain existed)"
                        .to_string(),
                });
            };
            let anchor = match &prev_hash {
                Some(prev) => prev.clone(),
                None => match genesis_from_first_row(run_id, &row) {
                    Ok(anchor) => anchor,
                    Err(error) => {
                        return Ok(ChainVerification::Broken {
                            seq,
                            detail: error.to_string(),
                        })
                    }
                },
            };
            let recomputed = chain_hash(ChainHashFields {
                prev_hash: &anchor,
                run_id: run_id.as_str(),
                seq: row.seq,
                ts: &row.ts,
                node_id: row.node_id.as_deref(),
                kind: &row.kind,
                payload_json: &row.payload_json,
                schema_version: row.schema_version,
            });
            if &recomputed != stored_hash {
                return Ok(ChainVerification::Broken {
                    seq,
                    detail: "stored hash does not match the recomputed chain — the event or \
                             its predecessor's hash was altered"
                        .to_string(),
                });
            }
            prev_hash = Some(stored_hash.clone());
            verified += 1;
        }

        if verified == 0 {
            return Err(StorageError::VerifyUnknownRun {
                run_id: run_id.clone(),
            });
        }
        Ok(ChainVerification::Intact { events: verified })
    }

    /// All events for a run, ordered by `seq` ascending — the order
    /// replay depends on. A row under a `kind` this binary does not know
    /// comes back as [`EventBody::Unknown`], kept verbatim; a row under a
    /// known kind whose payload is not that kind's shape is corrupt.
    pub fn events_for_run(&self, run_id: &RunId) -> Result<Vec<StoredEvent>> {
        let read_err = |error| StorageError::Read {
            run_id: run_id.clone(),
            source: cause(error),
        };

        let conn = lock(&self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT seq, ts, node_id, payload_json, schema_version
                 FROM events WHERE run_id = ?1 ORDER BY seq ASC",
            )
            .map_err(read_err)?;

        let rows = stmt
            .query_map(params![run_id.as_str()], |row| {
                let seq: i64 = row.get(0)?;
                let ts: String = row.get(1)?;
                let node_id: Option<String> = row.get(2)?;
                let payload_json: String = row.get(3)?;
                let schema_version: u32 = row.get(4)?;
                Ok((seq, ts, node_id, payload_json, schema_version))
            })
            .map_err(read_err)?;

        let mut events = Vec::new();
        for row in rows {
            let (seq, ts, node_id, payload_json, schema_version) = row.map_err(read_err)?;
            let seq = Seq::try_from(seq).map_err(|source| StorageError::CorruptSeq {
                run_id: run_id.clone(),
                source,
            })?;
            let corrupt = |source: EventShapeError| StorageError::CorruptPayload {
                run_id: run_id.clone(),
                seq,
                source,
            };
            let object = match serde_json::from_str::<serde_json::Value>(&payload_json) {
                Ok(serde_json::Value::Object(object)) => object,
                Ok(_) => return Err(corrupt(EventShapeError::NotAnObject)),
                Err(source) => return Err(corrupt(EventShapeError::Json { source })),
            };
            let body = EventBody::from_object(object, schema_version).map_err(corrupt)?;

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

            events.push(StoredEvent {
                run_id: run_id.clone(),
                seq,
                timestamp,
                node_id,
                body,
            });
        }
        Ok(events)
    }

    /// Deletes every row for `run_id` — the database side of
    /// `storage.retention_days`. The one deliberate exception to the
    /// append-only discipline, and the *caller* (`yunta gc`) owns the
    /// safety rule: rows die only after the run.dir (whose exported
    /// `events.jsonl` is the self-contained copy) is already gone — the
    /// database is never the first copy to die.
    pub fn purge_run(&self, run_id: &RunId) -> Result<Purge> {
        let conn = lock(&self.conn);
        let rows = conn
            .execute(
                "DELETE FROM events WHERE run_id = ?1",
                params![run_id.as_str()],
            )
            .map_err(|error| StorageError::Read {
                run_id: run_id.clone(),
                source: cause(error),
            })?;
        Ok(Purge { rows })
    }

    /// Every run with at least one event, oldest first by the timestamp
    /// of its first event (then by id), for `yunta list --runs`.
    pub fn list_runs(&self) -> Result<Vec<RunListing>> {
        let list_err = |error| StorageError::ListRuns {
            source: cause(error),
        };
        let conn = lock(&self.conn);
        let mut stmt = conn
            .prepare("SELECT run_id, ts FROM events WHERE seq = 1 ORDER BY ts ASC, run_id ASC")
            .map_err(list_err)?;

        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(list_err)?;

        let mut runs = Vec::new();
        for row in rows {
            let (run_id, ts) = row.map_err(list_err)?;
            let run_id =
                RunId::try_from(run_id).map_err(|source| StorageError::CorruptRunId { source })?;
            let created_at = chrono::DateTime::parse_from_rfc3339(&ts)
                .map_err(|source| StorageError::CorruptTimestamp {
                    run_id: run_id.clone(),
                    seq: Seq::FIRST,
                    source,
                })?
                .with_timezone(&chrono::Utc);
            runs.push(RunListing { run_id, created_at });
        }
        Ok(runs)
    }
}

/// One persisted row as `verify_chain` reads it back.
struct StoredRow {
    seq: i64,
    ts: String,
    node_id: Option<String>,
    kind: String,
    payload_json: String,
    schema_version: u32,
    event_hash: Option<String>,
}

/// The chain's anchor for a run's first row: `run_created`'s own
/// manifest hash, hashed.
fn genesis_from_first_row(run_id: &RunId, row: &StoredRow) -> Result<String> {
    if row.kind != "run_created" {
        return Err(StorageError::GenesisMissing {
            run_id: run_id.clone(),
        });
    }
    let manifest_hash = serde_json::from_str::<serde_json::Value>(&row.payload_json)
        .ok()
        .and_then(|value| {
            value
                .get("manifest_hash")
                .and_then(|hash| hash.as_str().map(String::from))
        });
    manifest_hash
        .map(|hash| genesis_hash(&hash))
        .ok_or_else(|| StorageError::CorruptGenesis {
            run_id: run_id.clone(),
        })
}
