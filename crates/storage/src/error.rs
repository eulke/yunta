use thiserror::Error;
use yunta_core::events::EventShapeError;
use yunta_core::{InvalidId, RunId, Seq};

/// The backend's own failure, kept as the cause of a [`StorageError`]
/// without naming the backend in this crate's API.
pub type Cause = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("failed to open the event log at {path}")]
    Open {
        path: std::path::PathBuf,
        #[source]
        source: Cause,
    },

    #[error("failed to append an event for run `{run_id}`")]
    Append {
        run_id: RunId,
        #[source]
        source: Cause,
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
        source: Cause,
    },

    /// A row whose `kind` this binary knows but whose payload is not that
    /// kind's shape — or is not JSON at all. An unknown kind is not this:
    /// the reader keeps it as [`yunta_core::events::EventBody::Unknown`].
    #[error("stored payload for run `{run_id}` seq {seq} is not an event")]
    CorruptPayload {
        run_id: RunId,
        seq: Seq,
        #[source]
        source: EventShapeError,
    },

    #[error("stored timestamp for run `{run_id}` seq {seq} is not valid RFC3339")]
    CorruptTimestamp {
        run_id: RunId,
        seq: Seq,
        #[source]
        source: chrono::ParseError,
    },

    #[error("stored node id for run `{run_id}` seq {seq} is not a node id")]
    CorruptNodeId {
        run_id: RunId,
        seq: Seq,
        #[source]
        source: InvalidId,
    },

    #[error("a stored seq for run `{run_id}` is not a position")]
    CorruptSeq {
        run_id: RunId,
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
        source: Cause,
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

    /// The first row is `run_created` but carries no readable
    /// `manifest_hash`, so the chain has no genesis to anchor to.
    #[error("run `{run_id}`: its `run_created` has no readable manifest_hash — no chain genesis")]
    CorruptGenesis { run_id: RunId },

    /// A blocking storage call could not be joined from the async
    /// runtime — the task that ran it was cancelled or panicked.
    #[error("the storage task to {what} did not complete: {detail}")]
    Join { what: &'static str, detail: String },
}

pub type Result<T> = std::result::Result<T, StorageError>;
