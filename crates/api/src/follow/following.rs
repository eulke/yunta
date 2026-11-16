//! See [`super`]. What is followed, followed while it moves, from a process that does
//! not drive it: the run read whole, what came past its head, the engine as the run's
//! registry reads it, what the run's sessions say, and the run a promotion hands the
//! work to; beside it the field, whose side of a look is `field`'s, what the run waits on
//! a person for, `awaited`'s, and what its nodes hand over, `document`'s. Every interface
//! that follows a run reads the same news at the same pace.
//!
//! A following never ends by itself: a run that parks, stalls or closes is a reading
//! of its frame, and the following reads on past it, as a parked run resumed elsewhere
//! moves again. It ends only by its caller's rule, the one every interface stops by,
//! `!frame.moves() && !following.awaits_successor()`, or by its stop. A look hands what
//! the log says before what the registry says, so a caller that stops at the rule
//! after a piece of news has been handed every event its frame reads; and a run closed
//! promoted whose successor does not come in the time one takes to be created is
//! handed as having none, once, so a caller waiting on the following reads its rule
//! again.
//!
//! What a look found waits in the following's queue until it is handed, and a look
//! changes what the following holds only once a read is done: a caller may drop
//! [`Following::next`] wherever it stands, to follow something else, and lose nothing.
//!
//! A following only reads: past the opening of its reader, which sets a fresh or older
//! log up as every open does, it takes no lock, writes no event, writes no registry and
//! never signals the engine. It owns what it reads, so a task drawing the run holds its
//! own.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::mpsc;
use tokio::time::Instant;
use yunta_core::pace::Pace;
use yunta_core::{Clock, RunId};
use yunta_engine::said::Heard;
use yunta_replay::frame::{RunFrame, RunPhase};
use yunta_replay::registry::Engine;
use yunta_storage::LogReader;

use super::awaited::Asking;
use super::field::HeldField;
use super::listening::Listening;
use super::{document::Handing, FollowNews, Followed, FollowedRun, RunNews, RunRegistry};
use crate::event_log::PromotionChain;
use crate::run::read_manifest;
use crate::{Error, Failure, OpenedRun, Project, Stop};

/// How often a following asks the log whether another connection committed since it
/// last asked. A tenth of a second puts an event on a follower's screen well inside the
/// quarter second a person reads as at once, and a log nothing was committed to costs
/// one pragma.
const READ_EVERY: Duration = Duration::from_millis(100);

/// How often a following reads the run's registry while the run's log says it moves or
/// asks a person. Once a second tells a gone engine within the two seconds a stall is
/// owed, with no read of the process table for every look.
pub(super) const REGISTRY_EVERY: Duration = Duration::from_secs(1);

/// How long a following awaits the successor of a run that closed promoted before it
/// hands that none came: the engine that promotes creates it once it has closed the run
/// and set its tree up, which takes a few seconds on a large repository.
pub(super) const SUCCESSOR_WAIT: Duration = Duration::from_secs(30);

/// What is followed, followed while it moves. Not the run's driving, which happens in
/// the process its registry names.
///
/// [`Following::next`] may be dropped wherever it stands, between two looks or inside
/// one, and loses no news: what a look found waits in the following's queue until it is
/// handed, so a caller cancels `next` to call [`Following::refollow`].
pub struct Following {
    /// The project followed in, whose runs a following reads whole.
    pub(super) project: Project,
    /// The one connection to the log every look reads through, kept open, and every read
    /// of what the run asks.
    pub(super) reader: Arc<LogReader>,
    /// The workspace's clock, which the run's frames are stamped by and nothing is
    /// paced on.
    clock: Arc<dyn Clock>,
    /// What the caller asked to follow, with the run a promotion handed the work to in
    /// its predecessor's place.
    pub(super) followed: Followed,
    /// The run followed, as far as it has been read; none until it is read whole.
    pub(super) held: Option<Held>,
    /// The field followed, as far as each run of it has been read; none until it is
    /// read whole.
    pub(super) field: Option<HeldField>,
    /// Whether another connection committed since a look last read all a commit asks of
    /// it: kept until then, so a look dropped half way leaves the reading to the next.
    commit_unread: bool,
    /// The engine the run's registry named at a look, when it changed, which is handed
    /// after that look's events.
    engine_read: Option<Engine>,
    /// When the registry was last read: it is read again once a second has passed.
    registry_read_at: Instant,
    /// When the run was last looked at, which the next look is spaced from.
    looked_at: Instant,
    listening: Listening,
    heard: mpsc::UnboundedReceiver<Heard>,
    /// What a look found and has not been handed back yet.
    pub(super) news: VecDeque<FollowNews>,
    /// What the run held waits on a person for, as far as it was worked out and read.
    pub(super) asking: Asking,
    /// What the run held handed over, as far as it was worked out and handed.
    pub(super) handing: Handing,
}

/// The run a following holds read whole, with what it reads beside the run's log.
pub(super) struct Held {
    /// The run as the following read it, which each piece of its news folds into.
    pub(super) run: FollowedRun,
    /// The registry the run keeps beside it, in its directory.
    pub(super) registry: RunRegistry,
    /// The chain of promotions the run's successor is found along.
    chain: PromotionChain,
    pub(super) successor: SuccessorWait,
}

/// Where a following stands on the successor of the run it holds.
#[derive(Debug, Clone, Copy)]
pub(super) enum SuccessorWait {
    /// The run held has not closed promoted, as far as it has been read.
    NotPromoted,
    /// The run held closed promoted, first read so at `since`, and no successor of it
    /// has been found: the log is asked for one after each commit.
    Awaited { since: Instant },
    /// None came within [`SUCCESSOR_WAIT`], which was handed once; one that comes later
    /// is still followed on.
    RanOut,
}

impl SuccessorWait {
    /// The wait begun, as the run's close is first read: whether it began now.
    fn begin(&mut self) -> bool {
        let began = matches!(self, SuccessorWait::NotPromoted);
        if began {
            *self = SuccessorWait::Awaited {
                since: Instant::now(),
            };
        }
        began
    }
}

impl Project {
    /// Follows what `followed` names from here: the field it names read whole and handed
    /// first, then the run it names read whole, then what comes, at one pace, until its
    /// caller stops.
    pub async fn follow(&self, followed: Followed) -> Result<Following, Error> {
        // The reader opens before the run is read, so an event committed while it is read
        // is a move a look reads, never one lost before the first look.
        let reader = LogReader::open(&self.storage_path)
            .await
            .map_err(Failure::new)?;
        let (ear, heard) = mpsc::unbounded_channel();
        let mut following = Following {
            project: self.clone(),
            reader: Arc::new(reader),
            clock: Arc::clone(&self.workspace.clock),
            followed,
            held: None,
            field: None,
            commit_unread: false,
            engine_read: None,
            registry_read_at: Instant::now(),
            looked_at: Instant::now(),
            listening: Listening::handing_to(ear),
            heard,
            news: VecDeque::new(),
            asking: Asking::new(),
            handing: Handing::new(),
        };
        // Nothing stops the first reading of the field's repository but dropping the
        // following, which takes the git it runs down with it.
        following.read_field_whole(&Stop::new()).await?;
        following.read_whole().await?;
        following.hand_over().await;
        following.ask(following.clock.now());
        Ok(following)
    }
}

impl Following {
    /// The next news of what is followed: what a look found, or what a session said, as
    /// soon as either comes. It never ends by itself: its caller stops where
    /// `!frame.moves() && !awaits_successor()`, or trips `stop`, which is
    /// [`Error::Stopped`].
    pub async fn next(&mut self, stop: &Stop) -> Result<FollowNews, Error> {
        loop {
            if let Some(news) = self.news.pop_front() {
                return Ok(news);
            }
            tokio::select! {
                biased;
                () = stop.work().cancelled() => return Err(Error::Stopped),
                Some(Heard { said, missed }) = self.heard.recv() => {
                    self.found(RunNews::Said { said, missed });
                }
                Some((turn, news)) = self.asking.read.recv() => self.asked_read(turn, news),
                // Words handed back between looks never hold the next look back.
                () = Pace::every(READ_EVERY).beat_after(self.looked_at) => {
                    self.looked_at = Instant::now();
                    self.look(stop).await?;
                }
            }
        }
    }

    /// Follows `followed` from here, keeping its reader, its paces and what waits to be
    /// handed. A run it names that is not the one held is read whole at the next look and
    /// handed as [`FollowNews::Whole`]; the run let go hands nothing more, nor do the
    /// words its sessions said that were not handed yet. A field at another reach is read
    /// whole at the next look and handed as [`FollowNews::Field`]; one let go hands
    /// nothing more.
    pub fn refollow(&mut self, followed: Followed) {
        if followed == self.followed {
            return;
        }
        self.followed = followed;
        if self.followed.field != self.field.as_ref().map(|field| field.reach) {
            self.field = None;
        }
        let holds = self.followed.run.is_some() && self.whole_owed().is_none();
        if !holds {
            self.held = None;
            self.engine_read = None;
            self.listening.on(None);
            self.asking.forget();
            self.handing.forget();
            while self.heard.try_recv().is_ok() {}
        }
    }

    /// Whether the run followed closed promoted and its successor has not come: none
    /// found yet, within the time one takes to be created since the close was first read,
    /// or one found, or none come, and not handed back yet. Each look past a commit asks
    /// the log for it; the wait holds no look.
    pub fn awaits_successor(&self) -> bool {
        let handing = self.news.iter().any(|news| {
            matches!(
                news,
                FollowNews::Run {
                    news: RunNews::Successor { .. } | RunNews::NoSuccessor,
                    ..
                }
            )
        });
        let awaited = self
            .held
            .as_ref()
            .is_some_and(|held| matches!(held.successor, SuccessorWait::Awaited { .. }));
        handing || awaited
    }

    /// One look: the registries when they are a second old and the field's directories
    /// when ten seconds old, whether the log moved and only then what came past each head
    /// held, the field and the run followed read whole when they are not the ones held,
    /// the run a promotion handed the work to, what the run's nodes handed over and what
    /// it asks once its head moved, and the socket the registry names while the run moves
    /// or asks. `stop` stops the git a field read whole asks.
    pub(super) async fn look(&mut self, stop: &Stop) -> Result<(), Error> {
        // The registry before the log: an engine that wrote its last event and left
        // between the two reads is read gone with that event, never as stalled.
        let now = self.clock.now();
        self.read_registry(now);
        self.read_field_registries(now);
        self.check_field_dirs();
        if self.reader.moved().await.map_err(Failure::new)? {
            self.commit_unread = true;
        }
        self.read_field_whole(stop).await?;
        self.read_whole().await?;
        if self.commit_unread {
            self.read_field_past_the_heads().await?;
            self.read_past_the_head().await?;
        }
        // Handed after the events, so a caller stopping at the rule once the events
        // are handed has every one its frame reads.
        if let Some(engine) = self.engine_read.take() {
            self.found(RunNews::Engine(engine));
        }
        self.hand_field_engines();
        self.onto_successors(now).await?;
        self.hand_over().await;
        self.ask(now);
        self.commit_unread = false;
        let socket = self
            .held
            .as_ref()
            .filter(|held| moves_or_asks(&held.run.frame(now)))
            .and_then(|held| held.run.engine.words());
        self.listening.on(socket);
        Ok(())
    }

    /// The run followed, when it is not the one held: the run to read whole.
    fn whole_owed(&self) -> Option<RunId> {
        let held = self.held.as_ref().map(|held| &held.run.run);
        self.followed.run.clone().filter(|run| held != Some(run))
    }

    /// The run followed read whole and handed, when it is not the one held: as the
    /// field reads it, where it does. Its chain of promotions begins before it is read,
    /// so a successor made while it is read is one the chain still finds.
    async fn read_whole(&mut self) -> Result<(), Error> {
        let Some(run) = self.whole_owed() else {
            return Ok(());
        };
        let chain = PromotionChain::starting_at(&self.reader, run.clone()).await?;
        let (read, registry, events) = match self.whole_from_the_field(&run).await? {
            Some(whole) => whole,
            None => {
                let open = self.project.run(&run).await?;
                let registry = open.registry();
                let OpenedRun {
                    id,
                    manifest,
                    events,
                    ..
                } = open;
                let read = FollowedRun::read(id, manifest.doc, &events, registry.engine());
                (read, registry, events)
            }
        };
        // A run already closed promoted is awaited a successor from here: one made since
        // the chain began is found by the first look past its commit.
        let mut successor = SuccessorWait::NotPromoted;
        if closed_promoted(&read.frame(self.clock.now())) {
            successor.begin();
        }
        self.held = Some(Held {
            run: read.clone(),
            registry,
            chain,
            successor,
        });
        self.engine_read = None;
        self.registry_read_at = Instant::now();
        self.news.push_back(FollowNews::Whole {
            run: Box::new(read),
            events,
        });
        Ok(())
    }

    /// What came past the head of the run held, kept to hand back: the run's whole log
    /// while none was read, as a run whose directory stands before its creation is
    /// logged. A run the field holds is the field's to read.
    async fn read_past_the_head(&mut self) -> Result<(), Error> {
        let Some(held) = &self.held else {
            return Ok(());
        };
        let run = &held.run.run;
        if self.field_holds(run) {
            return Ok(());
        }
        let past = match held.run.state.head() {
            Some(head) => self.reader.events_after(run, head).await,
            None => self.reader.events(run).await,
        };
        let past = past.map_err(Failure::new)?;
        if !past.is_empty() {
            self.found(RunNews::Events(past));
        }
        Ok(())
    }

    /// The engine the registry of the run held names, kept to hand when it changed: read
    /// again once the last reading is a second old, while the run's log says it moves or
    /// asks a person. `now` frames the run; the second is told on tokio's time. A run the
    /// field holds is the field's to read.
    fn read_registry(&mut self, now: DateTime<Utc>) {
        let Some(held) = &self.held else {
            return;
        };
        if self.field_holds(&held.run.run) {
            return;
        }
        if self.registry_read_at.elapsed() < REGISTRY_EVERY {
            return;
        }
        if !moves_or_asks(&held.run.frame(now)) {
            return;
        }
        self.registry_read_at = Instant::now();
        let engine = held.registry.engine();
        self.engine_read = (engine != held.run.engine).then_some(engine);
    }

    /// Follows the run held onto its successor while it stands closed promoted, and
    /// hands once that none came when the wait for one runs out. The log is asked as the
    /// close is first read, then after each commit; a successor found closed promoted in
    /// turn is asked after at once, along the chain.
    async fn onto_successors(&mut self, now: DateTime<Utc>) -> Result<(), Error> {
        loop {
            let Some(held) = self.held.as_mut() else {
                return Ok(());
            };
            if !closed_promoted(&held.run.frame(now)) {
                return Ok(());
            }
            if !held.successor.begin() && !self.commit_unread {
                break;
            }
            // The chain is asked as a copy, kept once the successor is read whole, so a
            // look dropped half way asks again.
            let mut chain = held.chain.clone();
            let Some(successor) = chain.successor(&self.reader).await? else {
                break;
            };
            self.go_on_with(successor, chain).await?;
        }
        self.ran_out();
        Ok(())
    }

    /// Follows `successor` from here on, read whole, as the field reads it where it does,
    /// else beside the run it was promoted from, since the engine that promotes creates
    /// it under the runs root it works in; `chain` is the chain that found it.
    async fn go_on_with(&mut self, successor: RunId, chain: PromotionChain) -> Result<(), Error> {
        let Some(held) = &self.held else {
            return Ok(());
        };
        let predecessor = held.run.run.clone();
        let (read, registry, events) = match self.whole_from_the_field(&successor).await? {
            Some(whole) => whole,
            None => {
                let registry = held.registry.of_successor(&successor);
                let manifest_path = yunta_engine::run_dir::manifest_path(registry.run_dir());
                let manifest = read_manifest(&manifest_path)?.doc;
                let events = self.reader.events(&successor).await.map_err(Failure::new)?;
                let read =
                    FollowedRun::read(successor.clone(), manifest, &events, registry.engine());
                (read, registry, events)
            }
        };
        self.held = Some(Held {
            run: read.clone(),
            registry,
            chain,
            successor: SuccessorWait::NotPromoted,
        });
        if self.followed.run.as_ref() == Some(&predecessor) {
            self.followed.run = Some(successor);
        }
        self.engine_read = None;
        self.registry_read_at = Instant::now();
        let news = RunNews::Successor {
            run: Box::new(read),
            events,
        };
        self.news.push_back(FollowNews::Run {
            run: predecessor,
            news,
        });
        Ok(())
    }

    /// That no successor came, kept to hand back once, when the run held has awaited one
    /// for [`SUCCESSOR_WAIT`].
    fn ran_out(&mut self) {
        let Some(held) = self.held.as_mut() else {
            return;
        };
        let SuccessorWait::Awaited { since } = held.successor else {
            return;
        };
        if since.elapsed() >= SUCCESSOR_WAIT {
            held.successor = SuccessorWait::RanOut;
            self.found(RunNews::NoSuccessor);
        }
    }

    /// `news` of the run held folded into it, and kept to hand back.
    pub(super) fn found(&mut self, news: RunNews) {
        let Some(held) = &self.held else {
            return;
        };
        let run = held.run.run.clone();
        self.found_of(run, news);
    }
}

/// Whether the run `frame` reads closed promoted, handing the work to a successor.
fn closed_promoted(frame: &RunFrame) -> bool {
    matches!(frame.phase, RunPhase::Promoted { .. })
}

/// Whether the run's log, as `frame` reads it, says it moves or asks a person: the runs
/// whose registry can tell what the log cannot, an engine that died or a question asked
/// at a terminal, neither of which commits anything. A run its log closed or broke has
/// no engine left to tell of, nor words to hear.
pub(super) fn moves_or_asks(frame: &RunFrame) -> bool {
    matches!(
        frame.phase,
        RunPhase::Created | RunPhase::Running | RunPhase::Waiting { .. }
    )
}
