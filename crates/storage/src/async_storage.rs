//! Storage from async code: every call runs on a blocking thread through
//! `spawn_blocking`, on a connection opened for that call from the log's
//! path — the runtime's threads never wait on SQLite. `Storage` itself
//! stays synchronous; a synchronous command opens it directly, and this
//! handle is the only way async code reaches the log.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use yunta_core::events::{EventDraft, StoredEvent};
use yunta_core::{RunId, Seq};

use crate::error::{Result, StorageError};
use crate::store::{ChainVerification, Purge, RunListing, Storage};

/// A handle to the event log for async callers: the log's path, and
/// nothing open. Cloning is cheap; each call opens and closes its own
/// connection on a blocking thread.
#[derive(Debug, Clone)]
pub struct AsyncStorage {
    path: PathBuf,
}

impl AsyncStorage {
    /// Opens the log at `path` once, on a blocking thread — creating the
    /// file and migrating its schema exactly as [`Storage::open`] does —
    /// and keeps only the path. A path that cannot be opened fails here,
    /// before the caller does anything else.
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let handle = AsyncStorage { path: path.into() };
        handle.blocking("open the log", |_| Ok(())).await?;
        Ok(handle)
    }

    /// The handle a [`Storage`] that already holds `path` open hands to
    /// async code; nothing is opened here.
    pub(crate) fn at(path: PathBuf) -> Self {
        AsyncStorage { path }
    }

    /// Runs `op` on a blocking thread, against a connection opened for
    /// it — the hop every method here takes, and the one a caller takes
    /// itself for a compound operation (a read over every run, say)
    /// that is one operation to it. `what` names the operation in the
    /// error when the blocking task cannot be joined.
    pub async fn blocking<T, F>(&self, what: &'static str, op: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Storage) -> Result<T> + Send + 'static,
    {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let storage = Storage::open(&path)?;
            op(&storage)
        })
        .await
        .map_err(|error| StorageError::Join {
            what,
            detail: error.to_string(),
        })?
    }

    /// Appends `draft`, stamped with `at` — the instant the caller's
    /// clock reported before the hop to the blocking thread.
    pub async fn append(&self, draft: EventDraft, at: DateTime<Utc>) -> Result<Seq> {
        self.blocking("append", move |storage| storage.append_at(&draft, at))
            .await
    }

    pub async fn events_for_run(&self, run_id: RunId) -> Result<Vec<StoredEvent>> {
        self.blocking("read a run's events", move |storage| {
            storage.events_for_run(&run_id)
        })
        .await
    }

    pub async fn verify_chain(&self, run_id: RunId) -> Result<ChainVerification> {
        self.blocking("verify a run's chain", move |storage| {
            storage.verify_chain(&run_id)
        })
        .await
    }

    pub async fn list_runs(&self) -> Result<Vec<RunListing>> {
        self.blocking("list runs", |storage| storage.list_runs())
            .await
    }

    pub async fn purge_run(&self, run_id: RunId) -> Result<Purge> {
        self.blocking("purge a run", move |storage| storage.purge_run(&run_id))
            .await
    }
}
