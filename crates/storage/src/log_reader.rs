//! The log read through one connection kept open: what a follower asks many
//! times a second, at the cost of one statement a call and never of an open.
//!
//! The reader is opened as [`Storage::open`] opens, so a first open on a fresh
//! or older log sets it up, taking the lock and writing as every open does;
//! then its connection is made to refuse every write, so a reader can never
//! change the log it follows. Each call runs one statement on a blocking
//! thread, and no transaction outlives a call, so a reader kept open never
//! holds the engine's checkpoint back. Not a read-only connection: one could
//! not set a fresh or older log up.

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;

use yunta_core::events::StoredEvent;
use yunta_core::{RunId, Seq};

use crate::error::{Result, StorageError};
use crate::store::{RunHead, Storage};

/// The log read through one connection kept open, which refuses every write
/// and says whether another connection committed since it last looked. Not
/// `AsyncStorage`, which opens a connection for each call.
#[derive(Debug)]
pub struct LogReader {
    /// The one connection, shared with the blocking thread each call runs on.
    storage: Arc<Storage>,
    /// The log's `data_version` as the reader last read it.
    seen: AtomicI64,
    /// How many statements the reader's calls ran since it opened.
    statements: AtomicU64,
}

impl LogReader {
    /// Opens the log at `path` on a blocking thread, as [`Storage::open`]
    /// does, then makes its connection refuse every write and reads where
    /// the log stands, so a first [`LogReader::moved`] answers for what came
    /// after the open.
    pub async fn open(path: impl Into<PathBuf>) -> Result<LogReader> {
        let path = path.into();
        let (storage, seen) = tokio::task::spawn_blocking(move || {
            let storage = Storage::open(&path)?;
            storage.refuse_writes()?;
            let seen = storage.data_version()?;
            Ok::<_, StorageError>((storage, seen))
        })
        .await
        .map_err(|error| StorageError::Join {
            what: "open the log for reading",
            detail: error.to_string(),
        })??;
        Ok(LogReader {
            storage: Arc::new(storage),
            seen: AtomicI64::new(seen),
            statements: AtomicU64::new(0),
        })
    }

    /// Whether another connection committed to the log since the reader
    /// last looked, or since it opened: one `PRAGMA data_version`, compared
    /// with the last reading. Two commits between two looks are one move.
    pub async fn moved(&self) -> Result<bool> {
        let version = self
            .run("read whether the log moved", Storage::data_version)
            .await?;
        Ok(self.seen.swap(version, Ordering::Relaxed) != version)
    }

    /// [`Storage::heads`], on the kept connection.
    pub async fn heads(&self) -> Result<Vec<RunHead>> {
        self.run("read each run's head", Storage::heads).await
    }

    /// [`Storage::events_after`], on the kept connection.
    pub async fn events_after(&self, run: &RunId, seq: Seq) -> Result<Vec<StoredEvent>> {
        let run = run.clone();
        self.run("read a run's events past a seq", move |storage| {
            storage.events_after(&run, seq)
        })
        .await
    }

    /// [`Storage::events_for_run`], on the kept connection.
    pub async fn events(&self, run: &RunId) -> Result<Vec<StoredEvent>> {
        let run = run.clone();
        self.run("read a run's events", move |storage| {
            storage.events_for_run(&run)
        })
        .await
    }

    /// [`Storage::first_events`], on the kept connection: each run's first
    /// event, in one statement, whatever the length of each run's log.
    pub async fn first_events(&self) -> Result<Vec<StoredEvent>> {
        self.run("read each run's first event", Storage::first_events)
            .await
    }

    /// How many statements the reader's calls ran since it opened, one a
    /// call: what a test of a follower counts to hold what a look costs.
    pub fn statements(&self) -> u64 {
        self.statements.load(Ordering::Relaxed)
    }

    /// Runs one statement on the kept connection, on a blocking thread, and
    /// counts it. `what` names the call in the error when the blocking task
    /// cannot be joined.
    async fn run<T, F>(&self, what: &'static str, statement: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Storage) -> Result<T> + Send + 'static,
    {
        self.statements.fetch_add(1, Ordering::Relaxed);
        let storage = Arc::clone(&self.storage);
        tokio::task::spawn_blocking(move || statement(&storage))
            .await
            .map_err(|error| StorageError::Join {
                what,
                detail: error.to_string(),
            })?
    }
}

#[cfg(test)]
mod tests;
