//! Reading the log back: a run whole or past a seq, the runs the log holds, and
//! where each of them stands.
//!
//! A row becomes a [`StoredEvent`] in one place, so every read keeps a kind this
//! binary does not know verbatim and fails on a corrupt row the same way. A read
//! is one statement: nothing here holds a read transaction open between calls, so
//! a reader never keeps the writer's log from its checkpoint.

use rusqlite::{params, Connection, Row};
use yunta_core::events::{EventBody, EventShapeError, StoredEvent};
use yunta_core::{NodeId, RunId, Seq};

use super::{cause, lock, RunListing, Storage};
use crate::error::{Result, StorageError};

/// A run's every row, in the order replay reads them.
const WHOLE_RUN: &str = "SELECT seq, ts, node_id, payload_json, schema_version
     FROM events WHERE run_id = ?1 ORDER BY seq ASC";

/// A run's rows past a seq, in the same order.
const PAST_A_SEQ: &str = "SELECT seq, ts, node_id, payload_json, schema_version
     FROM events WHERE run_id = ?1 AND seq > ?2 ORDER BY seq ASC";

/// Each run's last seq, read off the `(run_id, seq)` key one run at a time: the
/// next run id past the last one, then that run's greatest seq. Two seeks a run,
/// however long the run, and exact for a run a binary from before `run_heads`
/// wrote with no head row. Not a scan of `events` grouped by run, which reads
/// every event to answer one row a run.
const HEADS: &str = "WITH RECURSIVE ids(run_id) AS (
         SELECT MIN(run_id) FROM events
         UNION ALL
         SELECT (SELECT MIN(run_id) FROM events WHERE run_id > ids.run_id) FROM ids
         WHERE ids.run_id IS NOT NULL
     )
     SELECT run_id, (SELECT MAX(seq) FROM events WHERE events.run_id = ids.run_id)
     FROM ids WHERE run_id IS NOT NULL";

/// Where one run stands: the seq of its last event. Not the run's state, which
/// is derived from its events; this only says whether there are more of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunHead {
    pub run_id: RunId,
    pub seq: Seq,
}

/// One row of `events` as it comes off the database, before it is checked.
struct RawRow {
    seq: i64,
    ts: String,
    node_id: Option<String>,
    payload_json: String,
    schema_version: u32,
}

impl Storage {
    /// All events for a run, ordered by `seq` ascending — the order
    /// replay depends on. A row under a `kind` this binary does not know
    /// comes back as [`EventBody::Unknown`], kept verbatim; a row under a
    /// known kind whose payload is not that kind's shape is corrupt.
    pub fn events_for_run(&self, run_id: &RunId) -> Result<Vec<StoredEvent>> {
        read(&lock(&self.conn), run_id, None)
    }

    /// The events of `run_id` after `seq`, in log order: what a reader that
    /// already folded the run up to `seq` has not seen. Read the way
    /// [`Storage::events_for_run`] reads them, so an older or newer kind and a
    /// corrupt row come back the same; a corrupt row at or below `seq` is not
    /// read at all. Empty for a run that has nothing past `seq`, or no events.
    pub fn events_after(&self, run_id: &RunId, seq: Seq) -> Result<Vec<StoredEvent>> {
        read(&lock(&self.conn), run_id, Some(seq))
    }

    /// Every run with at least one event and its last seq, by run id, in one
    /// read: what a reader that follows many runs compares to learn which of them
    /// moved, without reading any of them.
    pub fn heads(&self) -> Result<Vec<RunHead>> {
        let heads_err = |error| StorageError::ListRuns {
            source: cause(error),
        };
        let conn = lock(&self.conn);
        let mut stmt = conn.prepare(HEADS).map_err(heads_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(heads_err)?;
        let mut heads = Vec::new();
        for row in rows {
            let (run_id, seq) = row.map_err(heads_err)?;
            let run_id =
                RunId::try_from(run_id).map_err(|source| StorageError::CorruptRunId { source })?;
            let seq = Seq::try_from(seq).map_err(|source| StorageError::CorruptSeq {
                run_id: run_id.clone(),
                source,
            })?;
            heads.push(RunHead { run_id, seq });
        }
        Ok(heads)
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

/// The rows of `run_id`, every one or only those past `after`, as stored events.
fn read(conn: &Connection, run_id: &RunId, after: Option<Seq>) -> Result<Vec<StoredEvent>> {
    let read_err = |error| StorageError::Read {
        run_id: run_id.clone(),
        source: cause(error),
    };
    let (query, past) = match after {
        // A seq past what SQLite's integer holds has nothing after it in a log.
        Some(seq) => (PAST_A_SEQ, i64::try_from(seq.get()).unwrap_or(i64::MAX)),
        None => (WHOLE_RUN, 0),
    };
    let mut stmt = conn.prepare(query).map_err(read_err)?;
    let rows = match after {
        Some(_) => stmt.query_map(params![run_id.as_str(), past], row_of),
        None => stmt.query_map(params![run_id.as_str()], row_of),
    }
    .map_err(read_err)?;
    let mut events = Vec::new();
    for row in rows {
        events.push(stored(run_id, row.map_err(read_err)?)?);
    }
    Ok(events)
}

/// The columns of one row, as the database typed them.
fn row_of(row: &Row<'_>) -> rusqlite::Result<RawRow> {
    Ok(RawRow {
        seq: row.get(0)?,
        ts: row.get(1)?,
        node_id: row.get(2)?,
        payload_json: row.get(3)?,
        schema_version: row.get(4)?,
    })
}

/// One row as the event it records, or the way it is corrupt.
fn stored(run_id: &RunId, row: RawRow) -> Result<StoredEvent> {
    let seq = Seq::try_from(row.seq).map_err(|source| StorageError::CorruptSeq {
        run_id: run_id.clone(),
        source,
    })?;
    let corrupt = |source: EventShapeError| StorageError::CorruptPayload {
        run_id: run_id.clone(),
        seq,
        source,
    };
    let object = match serde_json::from_str::<serde_json::Value>(&row.payload_json) {
        Ok(serde_json::Value::Object(object)) => object,
        Ok(_) => return Err(corrupt(EventShapeError::NotAnObject)),
        Err(source) => return Err(corrupt(EventShapeError::Json { source })),
    };
    let body = EventBody::from_object(object, row.schema_version).map_err(corrupt)?;
    let timestamp = chrono::DateTime::parse_from_rfc3339(&row.ts)
        .map_err(|source| StorageError::CorruptTimestamp {
            run_id: run_id.clone(),
            seq,
            source,
        })?
        .with_timezone(&chrono::Utc);
    let node_id = row
        .node_id
        .map(NodeId::try_from)
        .transpose()
        .map_err(|source| StorageError::CorruptNodeId {
            run_id: run_id.clone(),
            seq,
            source,
        })?;
    Ok(StoredEvent {
        run_id: run_id.clone(),
        seq,
        timestamp,
        node_id,
        body,
    })
}
