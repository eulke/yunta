//! See [`crate`]. A project's event log as a follower reads it while a run
//! moves: a run's whole log, what came past the last seq it holds, and the
//! runs it holds. Owned and cloned, so a task drawing a run holds its own.

use std::path::Path;

use yunta_core::events::StoredEvent;
use yunta_core::{RunId, Seq};
use yunta_storage::AsyncStorage;

use crate::{Error, Failure, Project};

/// A project's event log, open for reading while its runs move.
///
/// Not the engine's writer of a run's log: an interface only ever reads one.
#[derive(Clone)]
pub struct EventLog {
    storage: AsyncStorage,
}

impl Project {
    /// This project's event log, open for reading while its runs move.
    pub async fn event_log(&self) -> Result<EventLog, Error> {
        EventLog::open(&self.storage_path).await
    }
}

impl EventLog {
    /// The event log at `path`.
    pub async fn open(path: &Path) -> Result<EventLog, Error> {
        let storage = AsyncStorage::open(path).await.map_err(Failure::new)?;
        Ok(EventLog { storage })
    }

    /// Run `run`'s whole log as it stands.
    pub async fn events(&self, run: &RunId) -> Result<Vec<StoredEvent>, Error> {
        let events = self.storage.events_for_run(run.clone()).await;
        Ok(events.map_err(Failure::new)?)
    }

    /// What run `run`'s log holds past `seq`.
    pub async fn events_after(&self, run: &RunId, seq: Seq) -> Result<Vec<StoredEvent>, Error> {
        let events = self.storage.events_after(run.clone(), seq).await;
        Ok(events.map_err(Failure::new)?)
    }

    /// Every run the log holds, by the head its log reached: what tells a
    /// follower that a run it had not seen has begun.
    pub async fn runs(&self) -> Result<Vec<RunId>, Error> {
        let heads = self.storage.heads().await.map_err(Failure::new)?;
        Ok(heads.into_iter().map(|head| head.run_id).collect())
    }
}
