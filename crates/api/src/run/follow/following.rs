//! See [`super`]. A run followed while it moves, from a process that does not drive
//! it: what came past the last event read, the engine as the run's registry reads it,
//! what the run's sessions say, the run a promotion hands the work to, and where it
//! ends. Every interface that follows a run reads the same news at the same pace and
//! stops where the others do.
//!
//! A following only reads: it takes no lock, writes no event, writes no registry and
//! never signals the engine. It owns what it reads, so a task drawing the run holds its
//! own.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use surco::words::Standing;
use tokio::sync::mpsc;
use yunta_core::events::StoredEvent;
use yunta_core::pace::Pace;
use yunta_core::port::Said;
use yunta_core::{Clock, RunId};
use yunta_replay::frame::{RunFrame, RunPhase};
use yunta_replay::registry::Engine;

use super::listening::Listening;
use super::{FollowedRun, RunRegistry};
use crate::event_log::{EventLog, PromotionChain};
use crate::run::read_manifest;
use crate::{Error, Project, Stop};

/// How often a following asks the log what came past the last event it read. A tenth
/// of a second puts an event on a follower's screen well inside the quarter second a
/// person reads as at once, and a run that did not move costs one indexed read that
/// returns nothing.
const READ_EVERY: Duration = Duration::from_millis(100);

/// How often a following reads the run's registry. Once a second tells a gone engine
/// within the two seconds a stall is owed, with no read of the process table for every
/// read of the log.
const REGISTRY_EVERY: Duration = Duration::from_secs(1);

/// How long a following waits for the successor of a run that closed promoted: the
/// engine that promotes creates it once it has closed the run and set its tree up,
/// which takes a few seconds on a large repository.
const SUCCESSOR_WAIT: Duration = Duration::from_secs(30);

/// What a look at a followed run found. Not what a run's observer is handed in the
/// process that drives it, which sees each step as it is taken.
#[derive(Debug, Clone)]
pub enum RunNews {
    /// What came past the last event read, in the log's order.
    Events(Vec<StoredEvent>),
    /// The process driving the run, as its registry reads now that it changed.
    Engine(Engine),
    /// What one of the run's sessions said.
    Said(Said),
    /// The run a promotion handed the work to, followed from here on, as its log
    /// stands.
    Successor(Box<FollowedRun>),
    /// The run no longer moves.
    Stopped,
    /// Its log says it moves, and the engine its registry names is gone.
    Stalled,
}

/// A run followed while it moves. Not the run's driving, which happens in the process
/// its registry names.
pub struct Following {
    log: EventLog,
    clock: Arc<dyn Clock>,
    chain: PromotionChain,
    /// The run followed now, as far as it has been read.
    followed: FollowedRun,
    /// The directory the run followed now keeps its manifest and documents in.
    dir: PathBuf,
    /// The registry the run followed now keeps beside it.
    registry: RunRegistry,
    /// When the registry was last read: it is read again once a second has passed.
    registry_read_at: DateTime<Utc>,
    listening: Listening,
    heard: mpsc::UnboundedReceiver<Said>,
    /// What a look found and has not been handed back yet.
    news: VecDeque<RunNews>,
    /// Whether the run stopped or stalled, after which nothing more is handed back.
    ended: bool,
}

/// How following a run ends, read off its frame.
enum Ending {
    Stopped,
    Stalled,
}

impl Project {
    /// Follows run `id` from here: its log read whole, and a following that hands back
    /// what comes, at one pace, until the run stops.
    pub async fn follow(&self, id: &RunId) -> Result<Following, Error> {
        let open = self.run(id).await?;
        let log = self.event_log().await?;
        let chain = PromotionChain::starting_at(&log, id.clone()).await?;
        let registry = open.registry();
        let followed =
            FollowedRun::read(open.id, open.manifest.doc, open.events, registry.engine());
        let (ear, heard) = mpsc::unbounded_channel();
        Ok(Following {
            log,
            clock: Arc::clone(&self.workspace.clock),
            chain,
            followed,
            dir: open.dir,
            registry,
            registry_read_at: self.workspace.clock.now(),
            listening: Listening::handing_to(ear),
            heard,
            news: VecDeque::new(),
            ended: false,
        })
    }
}

impl Following {
    /// The next news of the run: what a look at it found, or what a session said, as
    /// soon as either comes. `None` once the run stopped or stalled; the caller's stop
    /// tripped is [`Error::Stopped`].
    pub async fn next(&mut self, stop: &Stop) -> Result<Option<RunNews>, Error> {
        loop {
            if let Some(news) = self.news.pop_front() {
                return Ok(Some(news));
            }
            if self.ended {
                return Ok(None);
            }
            tokio::select! {
                biased;
                () = stop.work().cancelled() => return Err(Error::Stopped),
                Some(said) = self.heard.recv() => return Ok(Some(RunNews::Said(said))),
                () = Pace::every(READ_EVERY).beat() => self.look(stop).await?,
            }
        }
    }

    /// The run followed now, as far as it has been read: the one asked for, or the
    /// last a promotion handed the work to.
    pub fn followed(&self) -> &FollowedRun {
        &self.followed
    }

    /// The directory the run followed now keeps its manifest and documents in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The registry the run followed now keeps beside it.
    pub fn registry(&self) -> &RunRegistry {
        &self.registry
    }

    /// The log the run is read from.
    pub fn log(&self) -> &EventLog {
        &self.log
    }

    /// The run's frame now, by the following's clock, from what was read and the
    /// engine as last read.
    pub fn frame(&self) -> RunFrame {
        self.followed.frame(self.clock.now())
    }

    /// One look at the run: what came past the last event read, the registry when it is
    /// a second old, the socket it names, and whether the run still moves. A run that
    /// closed promoted is followed onto its successor.
    async fn look(&mut self, stop: &Stop) -> Result<(), Error> {
        let past = self
            .log
            .events_after(&self.followed.run, self.followed.last())
            .await?;
        if !past.is_empty() {
            self.found(RunNews::Events(past));
        }
        let now = self.clock.now();
        self.read_registry(now);
        self.listening.on(self.followed.engine.words());
        let frame = self.followed.frame(now);
        let promoted = matches!(frame.phase, RunPhase::Promoted { .. });
        match ending(&frame) {
            None => {}
            Some(Ending::Stopped) if promoted => self.onto_successor(stop).await?,
            Some(Ending::Stopped) => self.end(RunNews::Stopped),
            Some(Ending::Stalled) => self.end(RunNews::Stalled),
        }
        Ok(())
    }

    /// Reads the registry again once it is a second old, and tells when the engine it
    /// names changed.
    fn read_registry(&mut self, now: DateTime<Utc>) {
        let stale = (now - self.registry_read_at)
            .to_std()
            .is_ok_and(|age| age >= REGISTRY_EVERY);
        if !stale {
            return;
        }
        self.registry_read_at = now;
        let engine = self.registry.engine();
        if engine != self.followed.engine {
            self.found(RunNews::Engine(engine));
        }
    }

    /// Waits for the successor of a run that closed promoted and goes on with it, or
    /// ends as stopped when none comes.
    async fn onto_successor(&mut self, stop: &Stop) -> Result<(), Error> {
        let since = self.clock.now();
        let waited = |clock: &dyn Clock| (clock.now() - since).to_std().unwrap_or_default();
        while waited(self.clock.as_ref()) < SUCCESSOR_WAIT {
            if let Some(successor) = self.chain.successor(&self.log).await? {
                return self.go_on_with(successor).await;
            }
            tokio::select! {
                biased;
                () = stop.work().cancelled() => return Err(Error::Stopped),
                () = Pace::every(READ_EVERY).beat() => {}
            }
        }
        self.end(RunNews::Stopped);
        Ok(())
    }

    /// Follows `successor` from here on, beside the run it was promoted from, since the
    /// engine that promotes creates it under the runs root it works in.
    async fn go_on_with(&mut self, successor: RunId) -> Result<(), Error> {
        let dir = self.dir.with_file_name(successor.as_str());
        let manifest = read_manifest(&yunta_engine::run_dir::manifest_path(&dir))?.doc;
        let events = self.log.events(&successor).await?;
        let registry = self.registry.of_successor(&successor);
        let followed = FollowedRun::read(successor, manifest, events, registry.engine());
        self.dir = dir;
        self.registry = registry;
        self.registry_read_at = self.clock.now();
        self.found(RunNews::Successor(Box::new(followed)));
        Ok(())
    }

    /// `news` folded into the run followed, and kept to hand back.
    fn found(&mut self, news: RunNews) {
        self.followed.took(&news);
        self.news.push_back(news);
    }

    /// Hands back `news` as the last.
    fn end(&mut self, news: RunNews) {
        self.listening.on(None);
        self.news.push_back(news);
        self.ended = true;
    }
}

/// How following a run `frame` reads ends, or `None` while the run moves. An absent
/// registry is not a stall: a run handed to a detached process has none for an instant.
fn ending(frame: &RunFrame) -> Option<Ending> {
    match (frame.moves(), frame.standing) {
        (true, _) => None,
        (false, Standing::Stalled) => Some(Ending::Stalled),
        (false, _) => Some(Ending::Stopped),
    }
}
