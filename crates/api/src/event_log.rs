//! See [`crate`]. A project's event log as an interface reads it while a run
//! moves: a run's whole log, or its log past a head the reader holds, through a
//! connection opened for the read; and the run a promotion created from a
//! followed run, read through the follower's own reader. Owned and cloned, so a
//! task drawing a run holds its own.

use std::collections::BTreeSet;
use std::path::Path;

use yunta_core::events::{EventPayload, RunEvent, StoredEvent};
use yunta_core::{RunId, Seq};
use yunta_storage::{AsyncStorage, LogReader};

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

    /// Run `run`'s log past `head`, as it stands: what a reader that holds the
    /// run up to `head` lacks, read without the events it holds.
    pub async fn events_after(&self, run: &RunId, head: Seq) -> Result<Vec<StoredEvent>, Error> {
        let events = self.storage.events_after(run.clone(), head).await;
        Ok(events.map_err(Failure::new)?)
    }
}

/// The run a promotion created from `run`, as `reader` reads the log now: the
/// run whose own `run_created` names `run` as the one it was promoted from,
/// among those `chain` does not hold, the runs already followed along the
/// promotions `run` ends; `None` while the log holds none.
///
/// A predecessor's log never names its successor; the successor's first event
/// does. So each time a run closed promoted is asked after, every run of the
/// log is asked by its first event, all read in one statement through the
/// reader its follower keeps, and none whole: a successor already in the log,
/// or one made between two looks, is found as one that comes, and so is each
/// run after it along a chain, in whatever order the log lists them. The log
/// is untrusted: a run whose birth names itself, or a cycle, would name a run
/// the chain holds already, which is no successor, so the chain stops there.
/// Not a list of runs counted between two asks, where a run counted once could
/// hide a successor. Every interface that follows a run goes on along the same
/// promotions.
pub(crate) async fn successor_of(
    reader: &LogReader,
    run: &RunId,
    chain: &BTreeSet<RunId>,
) -> Result<Option<RunId>, Error> {
    let firsts = reader.first_events().await.map_err(Failure::new)?;
    Ok(firsts.iter().find_map(|first| match first.payload() {
        Some(EventPayload::Run(RunEvent::Created(created)))
            if created.promoted_from.as_ref() == Some(run) && !chain.contains(&first.run_id) =>
        {
            Some(first.run_id.clone())
        }
        _ => None,
    }))
}
