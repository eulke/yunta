//! What the painter is told, and by whom.
//!
//! Two things reach it. The engine hands every append to
//! [`RunObserver::observe`] inline, on its own task, while the run
//! waits: nothing here may make it wait, so a full queue drops the
//! frame instead, because the frame is a copy of an event the log
//! already holds durably. What a drop costs is one seq in the painter's
//! stream, and a seq that stays missing is what sends the painter back
//! to the log.
//!
//! The other is whatever else draws on the same stream. A prompt and
//! the region cannot both have the terminal, so they take turns: the
//! [`Curtain`] is what a prompt asks with, and it waits for the painter
//! to say the rows are off the screen before it draws its first one.

use std::sync::Arc;

use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::watch;
use yunta_core::events::{EventBody, StoredEvent};
use yunta_engine::{Observed, RunObserver};

/// One thing the painter is told, in the order it was told.
pub(super) enum Beat {
    /// An event the engine appended, already at its position on the log.
    Event(Box<StoredEvent>),
    /// Nothing more is coming: fold what is queued ahead of this, then
    /// take the region down.
    Close,
}

/// The sender half, as the engine sees it.
///
/// Handed to `RunEnv.observer`, cloned by every `RunCtx` under it, and
/// dropped with the invocation. It derives nothing and decides nothing:
/// it copies what it is given into the queue and returns.
pub(super) struct Feed {
    beats: Sender<Beat>,
}

impl Feed {
    /// A feed and the receiving end the painter owns.
    pub(super) fn open() -> (Arc<Self>, Receiver<Beat>) {
        let (beats, received) = channel(super::QUEUE_DEPTH);
        (Arc::new(Self { beats }), received)
    }

    /// Sends a beat the surface itself raises, waiting for room rather
    /// than dropping it: a close is the painter's instruction, not a
    /// repaint it can afford to miss. An error means the painter is
    /// already gone, which is the state the caller wanted.
    pub(super) async fn tell(&self, beat: Beat) {
        // A closed queue means the painter has already stopped, which is
        // the state every instruction here is asking for.
        drop(self.beats.send(beat).await);
    }

    /// Whether the painter has taken every beat sent so far off the
    /// queue — where a test that asserts on what the painter did with a
    /// beat waits for it to have done it.
    #[cfg(test)]
    pub(super) fn taken(&self) -> bool {
        self.beats.capacity() == self.beats.max_capacity()
    }
}

impl RunObserver for Feed {
    fn observe(&self, event: Observed<'_>) {
        match self.beats.try_send(Beat::Event(Box::new(stored(event)))) {
            Ok(()) => {}
            // The queue's policy, not a failure: the frame copies an
            // event the log already holds, so the painter recovers it by
            // reading the log rather than by this call waiting here.
            Err(TrySendError::Full(_) | TrySendError::Closed(_)) => {}
        }
    }
}

/// The observed event in the shape a reader gets off the log, which is
/// what every derivation takes.
///
/// The body is always [`EventBody::Known`]: the event was written by this
/// binary a moment ago, so there is no newer writer's kind to keep
/// verbatim.
fn stored(event: Observed<'_>) -> StoredEvent {
    StoredEvent {
        run_id: event.run_id.clone(),
        seq: event.seq,
        timestamp: event.at,
        node_id: event.node_id.cloned(),
        body: EventBody::Known(event.payload.clone()),
    }
}

/// Whether the surface may draw.
///
/// The region and a prompt write to the same stream, and a region
/// redrawn over an open prompt lands on the rows a person is reading.
/// So they take turns: the surface stands down before the prompt draws
/// its first row, and comes back once the prompt has ended — answered,
/// declined, interrupted or unreadable alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Standing {
    /// The surface draws.
    Drawing,
    /// The surface draws nothing, and its region is off the terminal.
    StoodDown,
}

/// One request to the surface, numbered.
///
/// The number is what an acknowledgement names. Two requests carry the
/// same standing whenever one prompt ends and another opens, so a
/// waiter that watched the standing alone would take the answer to the
/// older request for the answer to its own and let a row land on the
/// prompt it was opening.
#[derive(Clone, Copy, Debug)]
pub(super) struct Request {
    pub(super) at: u64,
    pub(super) standing: Standing,
}

/// The painter's half of the curtain: what it is asked for, and where
/// it says it has done it.
pub(super) struct Standby {
    pub(super) asked: watch::Receiver<Request>,
    pub(super) settled: watch::Sender<u64>,
}

/// The two halves of one stand-down, held together so every clone of a
/// curtain asks on the same one.
struct Taking {
    asked: watch::Sender<Request>,
    settled: watch::Receiver<u64>,
}

/// Tells the surface to stand down while something else has the
/// terminal, and to come back when it is done.
///
/// Cheap to clone, and blind to the run: what it carries is the
/// request, never the region and never what the region says. A curtain
/// over a surface that draws nothing grants every request at once, so
/// the caller that opens a prompt has one path whether a run is being
/// drawn or not.
///
/// One turn at a time. The engine puts gates and questions to a person
/// one after another, and a terminal could not hold two prompts at once
/// anyway: a stand-down is always given back before the next is asked
/// for.
#[derive(Clone)]
pub(crate) struct Curtain(Option<Arc<Taking>>);

impl Curtain {
    /// A curtain and the half the painter watches.
    pub(super) fn open() -> (Self, Standby) {
        let (asked, watched) = watch::channel(Request {
            at: 0,
            standing: Standing::Drawing,
        });
        let (acknowledged, settled) = watch::channel(0);
        (
            Self(Some(Arc::new(Taking { asked, settled }))),
            Standby {
                asked: watched,
                settled: acknowledged,
            },
        )
    }

    /// A curtain over a surface with nothing to take down.
    pub(crate) fn none() -> Self {
        Self(None)
    }

    /// Takes the surface's rows off the terminal, and returns once they
    /// are off it.
    ///
    /// Nothing the surface draws is in flight when this returns, so
    /// whatever draws next has the terminal to itself until
    /// [`Curtain::raise`]. It never waits on a person: what it waits
    /// for is the painter's acknowledgement, which costs one redraw.
    pub(crate) async fn lower(&self) {
        let Some(taking) = &self.0 else { return };
        let mut at = 0;
        taking.asked.send_modify(|request| {
            request.at = request.at.saturating_add(1);
            request.standing = Standing::StoodDown;
            at = request.at;
        });
        let mut settled = taking.settled.clone();
        // A painter that has stopped acknowledges nothing and draws
        // nothing either, which is the state this asks for.
        drop(settled.wait_for(|acknowledged| *acknowledged >= at).await);
    }

    /// Gives the terminal back to the surface.
    ///
    /// It returns without waiting: what follows a prompt is the run
    /// carrying on, and every row drawn from here lands where it
    /// belongs.
    pub(crate) fn raise(&self) {
        let Some(taking) = &self.0 else { return };
        taking.asked.send_modify(|request| {
            request.at = request.at.saturating_add(1);
            request.standing = Standing::Drawing;
        });
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{EventPayload, NodeStartedPayload};
    use yunta_core::{
        Clock, CommitSha, ConfigLayer, ContentHash, Isolation, Manifest, RunId, Seq, SystemClock,
    };
    use yunta_storage::AsyncStorage;
    use yunta_testkit::{wait_until_async, Captured, FixedClock};

    use crate::render::Glyphs;
    use crate::surface::painter::{paint, Draw, Painter};
    use crate::surface::region::Region;
    use crate::surface::{Delivery, Screen, SurfaceEnv, Watched};

    use super::*;

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");

    /// What a prompt draws on the terminal it was handed. Its exact text
    /// is what the assertions look for: anything else on the screen came
    /// from the surface.
    const PROMPT: &str = "a decision is needed";

    /// A run frozen with nothing in it. What these tests read is the
    /// region's own rows, which every run has whatever its workflow
    /// declares.
    fn manifest() -> Manifest {
        Manifest {
            schema_version: 1,
            yunta_version: "0.0.0".to_string(),
            workflow: serde_norway::from_str("name: paced\nnodes: []\n")
                .expect("a workflow with no nodes parses"),
            config: ConfigLayer::default(),
            inputs: std::collections::BTreeMap::new(),
            prompts: std::collections::BTreeMap::new(),
            base_branch: "main".to_string(),
            base_commit: CommitSha::from_static("0123456789abcdef0123456789abcdef01234567"),
            isolation: Isolation::None,
            max_parallel_nodes: 1,
            workflow_hash: ContentHash::sha256(b"workflow"),
            config_hash: ContentHash::sha256(b"config"),
            paths: None,
            pack: None,
        }
    }

    fn event(seq: u64) -> Beat {
        Beat::Event(Box::new(StoredEvent {
            run_id: RUN.clone(),
            seq: Seq::try_from(seq as i64).expect("a positive seq"),
            timestamp: Clock::now(&FixedClock),
            node_id: None,
            body: EventBody::Known(EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 })),
        }))
    }

    /// A painter drawing a run, and everything that talks to it: the
    /// engine's side of the queue, the curtain a prompt takes its turn
    /// with, and the terminal both of them draw on.
    struct Drawing {
        feed: Arc<Feed>,
        curtain: Curtain,
        painter: tokio::task::JoinHandle<()>,
        screen: Watched,
        scrollback: Captured,
    }

    impl Drawing {
        /// A painter on `delivery`, already drawing.
        async fn open(delivery: Delivery, at: &std::path::Path) -> Self {
            let screen = Watched::sized(24, 80);
            let scrollback = Captured::default();
            let storage = AsyncStorage::open(at.join("events.db"))
                .await
                .expect("a store opens under the test's own directory");
            let manifest = manifest();
            let env = SurfaceEnv {
                run_id: &RUN,
                manifest: &manifest,
                prior: None,
                storage: &storage,
                clock: Arc::new(SystemClock),
                delivery,
                glyphs: Glyphs::Ascii,
            };
            let draw = match delivery {
                Delivery::Live => Draw::Live(Box::new(
                    Region::open(
                        Screen::immediate(screen.clone()),
                        Glyphs::Ascii,
                        Box::new(scrollback.clone()),
                    )
                    .expect("the region's row template parses"),
                )),
                _ => Draw::Lines(crate::surface::lines::Lines::open(
                    Box::new(scrollback.clone()),
                    "a test",
                )),
            };
            let (feed, beats) = Feed::open();
            let (curtain, standby) = Curtain::open();
            Self {
                feed,
                curtain,
                painter: tokio::spawn(paint(
                    Painter::seeded(&env, draw, Vec::new()),
                    beats,
                    standby,
                )),
                screen,
                scrollback,
            }
        }

        /// Everything on the terminal right now.
        fn shown(&self) -> String {
            self.screen.shown()
        }

        /// Waits until the painter has taken every beat sent so far.
        ///
        /// One pass takes the beats off the queue and folds, writes and
        /// redraws for them without yielding, so what the painter was
        /// going to do with them is done by the time this returns —
        /// which is what lets the assertion after it read "and it drew
        /// nothing" rather than "it has not drawn anything yet".
        async fn taken(&self) {
            wait_until_async(
                || async { self.feed.taken() },
                || "the painter never took what it was sent".to_string(),
            )
            .await;
        }

        /// Drives the painter to its end, which is the one moment every
        /// beat already queued has been taken: what it has not written
        /// by then, it never will.
        async fn drained(self) {
            self.feed.tell(Beat::Close).await;
            self.painter.await.expect("the painter ran to its end");
        }
    }

    #[tokio::test]
    async fn the_region_is_off_the_terminal_by_the_time_a_prompt_is_told_it_may_draw() {
        let at = tempfile::tempdir().expect("a directory for this test's store");
        let drawing = Drawing::open(Delivery::Live, at.path()).await;
        drawing.feed.tell(event(1)).await;
        wait_until_async(
            || async { !drawing.shown().is_empty() },
            || "the region never drew anything to take down".to_string(),
        )
        .await;

        drawing.curtain.lower().await;
        assert_eq!(
            drawing.shown(),
            "",
            "a row was still on the terminal when the prompt was told it could draw"
        );
    }

    #[tokio::test]
    async fn nothing_the_surface_draws_lands_on_a_prompt_that_is_open() {
        let at = tempfile::tempdir().expect("a directory for this test's store");
        let drawing = Drawing::open(Delivery::Live, at.path()).await;
        drawing.feed.tell(event(1)).await;
        wait_until_async(
            || async { !drawing.shown().is_empty() },
            || "the region never drew anything to take down".to_string(),
        )
        .await;
        drawing.curtain.lower().await;

        // The prompt now has the terminal, and the run keeps moving
        // under it: every event here is one the surface would redraw for.
        let before = drawing.screen.written();
        drawing
            .screen
            .interject(PROMPT)
            .expect("the terminal takes the prompt's own row");
        for seq in 2..=4 {
            drawing.feed.tell(event(seq)).await;
        }
        let screen = drawing.screen.clone();
        drawing.drained().await;

        assert_eq!(
            screen.written(),
            before,
            "the surface wrote to the terminal a person was answering on"
        );
    }

    #[tokio::test]
    async fn the_append_only_surface_holds_its_lines_while_a_prompt_is_open() {
        let at = tempfile::tempdir().expect("a directory for this test's store");
        let drawing = Drawing::open(Delivery::Lines { reason: "a test" }, at.path()).await;
        drawing.curtain.lower().await;
        let held = drawing.scrollback.text();
        for seq in 1..=3 {
            drawing.feed.tell(event(seq)).await;
        }
        drawing.taken().await;
        assert_eq!(
            drawing.scrollback.text(),
            held,
            "a line written under an open prompt lands on the rows a person is reading"
        );

        // Held, not dropped: the terminal comes back and so do they.
        drawing.curtain.raise();
        wait_until_async(
            || async { drawing.scrollback.text().len() > held.len() },
            || "the lines held for the prompt never went out after it ended".to_string(),
        )
        .await;
    }

    #[tokio::test]
    async fn a_surface_closing_under_a_prompt_still_writes_the_lines_it_held() {
        let at = tempfile::tempdir().expect("a directory for this test's store");
        let drawing = Drawing::open(Delivery::Lines { reason: "a test" }, at.path()).await;
        drawing.curtain.lower().await;
        let held = drawing.scrollback.text();
        for seq in 1..=3 {
            drawing.feed.tell(event(seq)).await;
        }

        // A run stopped from outside closes its surface with the prompt
        // it was asking on already over. Whatever the surface still owes
        // is owed to a terminal that is its own again.
        let scrollback = drawing.scrollback.clone();
        drawing.drained().await;
        assert!(
            scrollback.text().len() > held.len(),
            "the events settled under the prompt reached nobody: {:?}",
            scrollback.text()
        );
    }

    #[tokio::test]
    async fn the_region_comes_back_when_the_prompt_gives_the_terminal_up() {
        let at = tempfile::tempdir().expect("a directory for this test's store");
        let drawing = Drawing::open(Delivery::Live, at.path()).await;
        drawing.feed.tell(event(1)).await;
        drawing.curtain.lower().await;

        drawing.curtain.raise();
        wait_until_async(
            || async { drawing.shown().contains("nodes ") },
            || "the region never came back after the prompt ended".to_string(),
        )
        .await;
    }
}
