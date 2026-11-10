//! See [`crate`]. A project's event log as a follower reads it while a run
//! moves: a run's whole log, what came past the last seq it holds, the runs
//! it holds, and the chain of promotions a followed run goes on along. Owned
//! and cloned, so a task drawing a run holds its own.

use std::collections::HashSet;
use std::path::Path;

use yunta_core::events::{EventPayload, RunEvent, StoredEvent};
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

    /// The run a promotion created `run` from, as `run`'s own `run_created`
    /// records it; `None` for a run no promotion created.
    async fn promoted_from(&self, run: &RunId) -> Result<Option<RunId>, Error> {
        let events = self.events(run).await?;
        Ok(events.first().and_then(|first| match first.payload() {
            Some(EventPayload::Run(RunEvent::Created(created))) => created.promoted_from.clone(),
            _ => None,
        }))
    }
}

/// A followed run's chain of promotions: the run followed now, and every run
/// of the log already looked at, none of which can be its successor.
///
/// A predecessor's log never names its successor: the successor's own
/// `run_created` names the run it was promoted from. So the chain reads the
/// head of each run the log holds that it had not looked at, once, and every
/// interface that follows a run goes on along the same chain.
pub struct PromotionChain {
    followed: RunId,
    seen: HashSet<RunId>,
}

impl PromotionChain {
    /// The chain from `run`, as `log` holds it now: no run already on it is
    /// a successor of `run`.
    pub async fn starting_at(log: &EventLog, run: RunId) -> Result<PromotionChain, Error> {
        Ok(PromotionChain {
            followed: run,
            seen: log.runs().await?.into_iter().collect(),
        })
    }

    /// The run promoted from the one followed, once `log` holds it, which the
    /// chain follows from then on; `None` while there is none yet.
    pub async fn successor(&mut self, log: &EventLog) -> Result<Option<RunId>, Error> {
        for run in log.runs().await? {
            if !self.seen.insert(run.clone()) {
                continue;
            }
            if log.promoted_from(&run).await?.as_ref() == Some(&self.followed) {
                self.followed = run.clone();
                return Ok(Some(run));
            }
        }
        Ok(None)
    }
}
