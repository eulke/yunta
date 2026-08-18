use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};
use yunta_core::events::{Event, EventPayload};
use yunta_core::{NodeId, RunId};

use crate::error::{Result, StorageError};

/// T2.1's migration, embedded in the binary rather than shipped as a file
/// — the whole schema is one append-only table, so there is nothing a
/// migration runner would buy over `CREATE TABLE IF NOT EXISTS` today.
const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS events (
    run_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    ts TEXT NOT NULL,
    node_id TEXT,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    PRIMARY KEY (run_id, seq)
);
";

/// The event log (Contrato §3; D53 — SQLite is the only backend, and
/// nothing about it leaks past this interface: every method takes and
/// returns `yunta_core` types, never rusqlite's).
///
/// A single mutex-guarded connection serializes writes at the Rust level;
/// WAL mode (set in [`Storage::open`]) is what lets readers proceed
/// without blocking on that writer.
pub struct Storage {
    conn: Mutex<Connection>,
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
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(open_err)?;
        conn.execute_batch(SCHEMA_V1).map_err(open_err)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Appends one event, assigning the next `seq` for its `run_id`
    /// (append-only per I2 — nothing here ever updates or deletes a row).
    /// Returns the assigned `seq`.
    pub fn append_event(&self, event: &Event) -> Result<u64> {
        let append_err = |source| StorageError::Append {
            run_id: event.run_id.clone(),
            source,
        };

        let mut conn = lock(&self.conn);
        let tx = conn.transaction().map_err(append_err)?;

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

        tx.execute(
            "INSERT INTO events (run_id, seq, ts, node_id, kind, payload_json, schema_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.run_id.as_str(),
                seq,
                event.timestamp.to_rfc3339(),
                event.node_id.as_ref().map(NodeId::as_str),
                event.payload.kind_name(),
                payload_json,
                event.payload.schema_version(),
            ],
        )
        .map_err(append_err)?;

        tx.commit().map_err(append_err)?;

        Ok(seq as u64)
    }

    /// All events for a run, ordered by `seq` ascending — the order
    /// replay (T2.3) depends on.
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

            events.push(Event {
                run_id: run_id.clone(),
                seq,
                timestamp,
                node_id: node_id.map(Into::into),
                payload,
            });
        }
        Ok(events)
    }

    /// Every distinct `run_id` with at least one event, for `yunta run
    /// --runs` (T7.1) to enumerate.
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
            run_ids.push(RunId::from(run_id));
        }
        Ok(run_ids)
    }
}
