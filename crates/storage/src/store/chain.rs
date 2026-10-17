//! The chain a run's log is verified by: the verdict the walk gives, the walk
//! itself, and the stored head it has to end on.

use rusqlite::{params, OptionalExtension};
use yunta_core::{RunId, Seq};

use super::{cause, chain_hash, genesis_hash, lock, ChainHashFields, Storage};
use crate::error::{Result, StorageError};

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

impl Storage {
    /// Walks the run's whole chain recomputing every hash from the bytes
    /// as persisted, one row at a time: an altered payload, a deleted,
    /// inserted or reordered event, an altered stored hash, or a missing
    /// one all surface as [`ChainVerification::Broken`] naming the exact
    /// seq. The run's stored head then has to name the last row read, so a
    /// log whose tail was deleted, or a run with no head, is broken too.
    /// Runs automatically at receipt time and on demand via
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
            .query_map(params![run_id.as_str()], stored_row)
            .map_err(read_err)?;

        let mut prev_hash: Option<String> = None;
        let mut verified = 0usize;
        for row in rows {
            let row = row.map_err(read_err)?;
            if let Some(verdict) = judge_row(run_id, &row, verified, prev_hash.as_deref())? {
                return Ok(verdict);
            }
            prev_hash = row.event_hash;
            verified += 1;
        }

        if verified == 0 {
            return Err(StorageError::VerifyUnknownRun {
                run_id: run_id.clone(),
            });
        }
        let head = conn
            .query_row(
                "SELECT seq, event_hash FROM run_heads WHERE run_id = ?1",
                params![run_id.as_str()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(read_err)?;
        judge_head(run_id, head, verified, prev_hash.as_deref())
    }
}

/// Judges one row of the walk: `None` when it holds, else the broken verdict.
fn judge_row(
    run_id: &RunId,
    row: &StoredRow,
    verified: usize,
    prev_hash: Option<&str>,
) -> Result<Option<ChainVerification>> {
    let expected_seq = verified as i64 + 1;
    let seq = Seq::try_from(row.seq).map_err(|source| StorageError::CorruptSeq {
        run_id: run_id.clone(),
        source,
    })?;
    if row.seq != expected_seq {
        return Ok(broken(
            seq,
            format!(
                "expected seq {expected_seq} but found {} — an event was deleted, \
                 inserted or reordered",
                row.seq
            ),
        ));
    }
    let Some(stored_hash) = &row.event_hash else {
        return Ok(broken(
            seq,
            "event has no stored hash (written before the chain existed)",
        ));
    };
    let anchor = match prev_hash {
        Some(prev) => prev.to_string(),
        None => match genesis_from_first_row(run_id, row) {
            Ok(anchor) => anchor,
            Err(error) => return Ok(broken(seq, error.to_string())),
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
    if recomputed.as_str() != stored_hash.as_str() {
        return Ok(broken(
            seq,
            "stored hash does not match the recomputed chain — the event or \
             its predecessor's hash was altered",
        ));
    }
    Ok(None)
}

/// The verdict that the chain stops holding at `seq`, because of `detail`.
fn broken(seq: Seq, detail: impl Into<String>) -> Option<ChainVerification> {
    Some(ChainVerification::Broken {
        seq,
        detail: detail.into(),
    })
}

/// What the run's stored head says about a walk that verified `verified` rows
/// and ended on the row whose stored hash is `last_hash`. The head must name
/// that row: a log that stops short of its head has lost its tail, and a run
/// with no head has nothing to anchor its walk to.
fn judge_head(
    run_id: &RunId,
    head: Option<(i64, String)>,
    verified: usize,
    last_hash: Option<&str>,
) -> Result<ChainVerification> {
    // Every row was checked to be the next seq, so the last row read is the count.
    let last = verified as i64;
    let last_seq = Seq::try_from(last).map_err(|source| StorageError::CorruptSeq {
        run_id: run_id.clone(),
        source,
    })?;
    Ok(match head {
        Some((seq, hash)) if seq == last && Some(hash.as_str()) == last_hash => {
            ChainVerification::Intact { events: verified }
        }
        Some((seq, _)) if seq > last => ChainVerification::Broken {
            seq: Seq::try_from(seq).map_err(|source| StorageError::CorruptSeq {
                run_id: run_id.clone(),
                source,
            })?,
            detail: format!(
                "the log ends at seq {last}, its stored head is seq {seq}: \
                 the tail was deleted"
            ),
        },
        Some(_) => ChainVerification::Broken {
            seq: last_seq,
            detail: "the stored head does not name the last event".to_string(),
        },
        None => ChainVerification::Broken {
            seq: last_seq,
            detail: "no stored head: written before heads were anchored".to_string(),
        },
    })
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

/// Reads one row of the walk's query, columns in the order it selects them.
fn stored_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRow> {
    Ok(StoredRow {
        seq: row.get(0)?,
        ts: row.get(1)?,
        node_id: row.get(2)?,
        kind: row.get(3)?,
        payload_json: row.get(4)?,
        schema_version: row.get(5)?,
        event_hash: row.get(6)?,
    })
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
        .map(|hash| genesis_hash(&hash).to_string())
        .ok_or_else(|| StorageError::CorruptGenesis {
            run_id: run_id.clone(),
        })
}
