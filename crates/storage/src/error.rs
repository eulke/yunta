use thiserror::Error;
use yunta_core::{InvalidId, RunId};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("failed to open the event log at {path}")]
    Open {
        path: std::path::PathBuf,
        #[source]
        source: rusqlite::Error,
    },

    #[error("failed to append an event for run `{run_id}`")]
    Append {
        run_id: RunId,
        #[source]
        source: rusqlite::Error,
    },

    #[error("failed to serialize the payload for run `{run_id}`")]
    Serialize {
        run_id: RunId,
        #[source]
        source: serde_json::Error,
    },

    #[error("failed to read events for run `{run_id}`")]
    Read {
        run_id: RunId,
        #[source]
        source: rusqlite::Error,
    },

    #[error("stored payload for run `{run_id}` seq {seq} is not valid JSON")]
    CorruptPayload {
        run_id: RunId,
        seq: u64,
        #[source]
        source: serde_json::Error,
    },

    #[error("stored timestamp for run `{run_id}` seq {seq} is not valid RFC3339")]
    CorruptTimestamp {
        run_id: RunId,
        seq: u64,
        #[source]
        source: chrono::ParseError,
    },

    #[error("stored node id for run `{run_id}` seq {seq} is not a node id")]
    CorruptNodeId {
        run_id: RunId,
        seq: u64,
        #[source]
        source: InvalidId,
    },

    #[error("a stored run id is not a run id")]
    CorruptRunId {
        #[source]
        source: InvalidId,
    },

    #[error("failed to list runs")]
    ListRuns {
        #[source]
        source: rusqlite::Error,
    },

    /// The hash chain's genesis is `SHA-256(manifest_hash)`, and only
    /// `run_created` carries one — a run whose log starts with anything
    /// else has no chain to anchor, so the append is refused instead of
    /// hashed against an invented constant.
    #[error(
        "run `{run_id}`: the first event of a run must be `run_created` — the hash chain's \
         genesis is derived from its manifest_hash"
    )]
    GenesisMissing { run_id: RunId },

    #[error("run `{run_id}` has no events to verify")]
    VerifyUnknownRun { run_id: RunId },
}

pub type Result<T> = std::result::Result<T, StorageError>;
