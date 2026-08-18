use thiserror::Error;
use yunta_core::RunId;

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

    #[error("failed to list runs")]
    ListRuns {
        #[source]
        source: rusqlite::Error,
    },
}

pub type Result<T> = std::result::Result<T, StorageError>;
