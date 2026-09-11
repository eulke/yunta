//! The task that draws: it folds what the engine delivers, closes the
//! holes a dropped frame leaves, and redraws on its own beat so a
//! measured age keeps growing while nothing happens.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::Receiver;
use tokio::time::MissedTickBehavior;

use yunta_core::events::{EventPayload, StoredEvent, TerminalState};
use yunta_core::{Clock, Manifest, RunId};
use yunta_engine::{run_frame, PriorEstimation};
use yunta_storage::AsyncStorage;

use super::feed::{Beat, Standby, Standing};
use super::fold::Folded;
use super::lines::Lines;
use super::region::Region;
use super::SurfaceEnv;

/// How often the surface redraws on its own. The durations it shows are
/// counted in whole seconds, so a beat of one second is exactly as often
/// as any of them can change — anything faster redraws the same rows.
const REDRAW_INTERVAL: Duration = Duration::from_secs(1);

/// How many beats one pass takes off the queue: the queue's own depth, so
/// a burst that fits in it is folded together and drawn once instead of
/// redrawing the region for every event in it.
const BATCH: usize = super::QUEUE_DEPTH;

/// Where the folded run goes.
pub(super) enum Draw {
    Lines(Lines),
    Live(Box<Region>),
}

/// One invocation's drawing state: the run it is on, what it has of that
/// run's log, and the surface it puts it on.
pub(super) struct Painter {
    run_id: RunId,
    /// The manifest the run froze. A promotion successor inherits its
    /// predecessor's workflow whole, so the chain is drawn from this one
    /// — and it is what rebuilds the menu a parked run stopped on, which
    /// decides the command the demand line offers.
    manifest: Manifest,
    folded: Folded,
    draw: Draw,
    storage: AsyncStorage,
    clock: Arc<dyn Clock>,
    prior: Option<PriorEstimation>,
    /// Whether the hole in the stream was already there at the last beat.
    /// A hole is provisional on arrival — parallel nodes emit from
    /// different tasks and storage assigns the seq under its own lock, so
    /// a later event routinely arrives first — and only one that outlives
    /// a beat is a frame the queue dropped.
    stale_gap: bool,
    /// Whether this painter may draw at all. It stands down for as long
    /// as a prompt has the terminal, and what it would have drawn in
    /// the meantime is not lost: the region redraws from the run as it
    /// stands when it comes back, and the lines it owes are counted
    /// rather than queued.
    standing: Standing,
    /// How many settled events have gone out as lines.
    written: usize,
}

impl Painter {
    /// A painter on `env`'s run, holding the log as it stood when the
    /// surface opened.
    pub(super) fn seeded(env: &SurfaceEnv<'_>, draw: Draw, seed: Vec<StoredEvent>) -> Self {
        let mut painter = Self {
            run_id: env.run_id.clone(),
            manifest: env.manifest.clone(),
            folded: Folded::default(),
            draw,
            storage: env.storage.clone(),
            clock: Arc::clone(&env.clock),
            prior: env.prior.cloned(),
            stale_gap: false,
            standing: Standing::Drawing,
            written: 0,
        };
        painter.fold(seed);
        painter
    }

    /// Folds the events of the run being drawn and writes the line each
    /// newly settled one earns.
    fn fold(&mut self, events: Vec<StoredEvent>) {
        let Self { run_id, folded, .. } = self;
        for event in events {
            if &event.run_id == run_id {
                folded.fold(event);
            }
        }
        self.write_settled();
    }

    /// Writes a line for every event settled since the last one went
    /// out — what the append-only surface owes each of them, in the
    /// log's own order.
    ///
    /// Nothing goes out while the painter is stood down: a line printed
    /// under an open prompt lands on the rows a person is reading, and
    /// what is owed is counted rather than queued, so the order and the
    /// content survive the wait.
    fn write_settled(&mut self) {
        if self.standing == Standing::StoodDown {
            return;
        }
        let Self {
            folded,
            draw,
            written,
            ..
        } = self;
        if let Draw::Lines(lines) = draw {
            for event in folded.settled().iter().skip(*written) {
                lines.event(event);
            }
        }
        *written = folded.settled().len();
    }

    /// Takes one pass of events: the run's own are folded, and an event
    /// from the run that succeeded it moves the surface onto that one.
    ///
    /// Events under any other run id are left alone. A `kind: workflow`
    /// child keeps its own log under its own id, and what the run being
    /// drawn knows of it is on its own log already, as
    /// `child_run_created` and `child_run_finished`.
    async fn absorb(&mut self, events: Vec<StoredEvent>) {
        let foreign: Vec<RunId> = events
            .iter()
            .filter(|event| event.run_id != self.run_id)
            .map(|event| event.run_id.clone())
            .collect();
        self.fold(events);
        if let Some(successor) = foreign
            .into_iter()
            .find(|id| succeeds(self.folded.settled(), id))
        {
            self.follow(successor).await;
        }
    }

    /// Redraws the region from the run as it stands at this instant,
    /// unless the terminal is somebody else's for now.
    fn redraw(&mut self) {
        if self.standing == Standing::StoodDown {
            return;
        }
        let Self {
            run_id,
            manifest,
            folded,
            draw,
            clock,
            prior,
            ..
        } = self;
        let Draw::Live(region) = draw else { return };
        let frame = run_frame(
            run_id,
            &manifest.workflow,
            folded.settled(),
            prior.as_ref(),
            clock.now(),
        );
        // Only a parked run has a menu to rebuild, and rebuilding one is
        // a walk of the log — asked exactly when the answer changes what
        // the demand line says.
        let answerable = matches!(frame.phase, yunta_engine::RunPhase::Waiting { .. })
            && yunta_engine::current_escalation(manifest, folded.settled()).is_some();
        region.show(&frame, run_id, answerable);
    }

    /// One beat: recover a hole that outlived the last one, then redraw.
    async fn beat(&mut self) {
        if self.folded.gap() && self.stale_gap {
            self.reread().await;
        }
        self.stale_gap = self.folded.gap();
        self.redraw();
    }

    /// Reads the run's log and folds it whole — how a frame the queue
    /// dropped comes back. The log is the truth; this is the surface
    /// admitting its copy is behind.
    async fn reread(&mut self) {
        let Ok(events) = self.storage.events_for_run(self.run_id.clone()).await else {
            // A read that failed leaves the surface exactly as behind as
            // it was, and the next beat asks again.
            return;
        };
        self.folded.refill(events);
        self.write_settled();
    }

    /// Moves onto a promotion successor: a run of its own, with its own
    /// log, drawn as this same invocation carrying on.
    async fn follow(&mut self, run_id: RunId) {
        self.run_id = run_id;
        self.folded = Folded::default();
        self.stale_gap = false;
        self.written = 0;
        if let Draw::Live(region) = &mut self.draw {
            region.restart();
        }
        self.reread().await;
        self.redraw();
    }

    /// Takes the terminal for as long as a prompt needs it, or takes it
    /// back.
    ///
    /// Coming back is every line held in the meantime and then a
    /// redraw, in that order: the lines belong above the region, and
    /// the region being off the terminal is what puts them there.
    fn stand(&mut self, standing: Standing) {
        if self.standing == standing {
            return;
        }
        self.standing = standing;
        match standing {
            Standing::StoodDown => {
                if let Draw::Live(region) = &self.draw {
                    region.lower();
                }
            }
            Standing::Drawing => {
                self.write_settled();
                self.redraw();
            }
        }
    }

    /// Writes what the surface still owes and takes the region down, so
    /// what follows lands on a terminal with nothing pinned to it.
    ///
    /// The lines held for a prompt go out here even where nothing told
    /// the surface to come back. A prompt is over by the time the
    /// invocation closes its surface — the engine has its answer, or
    /// has stopped waiting for one — so the terminal is the surface's
    /// again, and an event whose line is still owed is an event missing
    /// from the only place it is written.
    fn finish(mut self) {
        self.standing = Standing::Drawing;
        self.write_settled();
        if let Draw::Live(region) = self.draw {
            region.close();
        }
    }

    /// Takes one pass of beats, answering whether the surface was told to
    /// close.
    async fn take(&mut self, beats: impl Iterator<Item = Beat>) -> bool {
        let mut events = Vec::new();
        let mut closing = false;
        for beat in beats {
            match beat {
                Beat::Event(event) => events.push(*event),
                Beat::Close => closing = true,
            }
        }
        self.absorb(events).await;
        self.redraw();
        closing
    }
}

/// Whether `run_id` is the promotion successor of the run whose log is
/// `events`.
///
/// Derived from the log rather than announced: a run closed `promoted`
/// executes nothing more, so the only run that can append after it is
/// the successor driven in its place — and the one other run that could
/// append under an id of its own, a `kind: workflow` child, is named on
/// this very log by the `child_run_created` that bore it.
fn succeeds(events: &[StoredEvent], run_id: &RunId) -> bool {
    let mut promoted = false;
    for event in events {
        match event.payload() {
            Some(EventPayload::RunFinished(p)) => {
                promoted = p.terminal_state == TerminalState::Promoted;
            }
            Some(EventPayload::ChildRunCreated(p)) if &p.child_run_id == run_id => return false,
            _ => {}
        }
    }
    promoted
}

/// Drives the painter until the surface says nothing more is coming.
///
/// Every beat queued ahead of [`Beat::Close`] is folded before the region
/// comes down, so the last thing left on the terminal is the run's real
/// last state rather than whatever the queue happened to have delivered.
pub(super) async fn paint(mut painter: Painter, mut beats: Receiver<Beat>, standby: Standby) {
    let Standby { mut asked, settled } = standby;
    let mut batch = Vec::with_capacity(BATCH);
    let mut beat = tokio::time::interval(REDRAW_INTERVAL);
    beat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // A curtain every holder has dropped asks for nothing more, and a
    // branch that watches one is ready forever.
    let mut taking_turns = true;
    loop {
        tokio::select! {
            received = beats.recv_many(&mut batch, BATCH) => {
                if painter.take(batch.drain(..)).await || received == 0 {
                    break;
                }
            }
            _ = beat.tick() => painter.beat().await,
            change = asked.changed(), if taking_turns => match change {
                Ok(()) => {
                    let request = *asked.borrow_and_update();
                    painter.stand(request.standing);
                    // Last, so a prompt that waits on this draws only
                    // once the rows are off the terminal.
                    settled.send_modify(|acknowledged| *acknowledged = request.at);
                }
                Err(_) => taking_turns = false,
            },
        }
    }
    painter.finish();
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{
        ChildRunCreatedPayload, EventBody, NodeStartedPayload, RunFinishedPayload, RunMetrics,
        TokenUsage,
    };
    use yunta_core::{ContentHash, Seq};
    use yunta_testkit::FixedClock;

    use super::*;

    const DRAWN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");
    const OTHER: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P6");

    fn event(seq: u64, payload: EventPayload) -> StoredEvent {
        StoredEvent {
            run_id: DRAWN.clone(),
            seq: Seq::try_from(seq as i64).expect("a positive seq"),
            timestamp: Clock::now(&FixedClock),
            node_id: None,
            body: EventBody::Known(payload),
        }
    }

    fn started() -> EventPayload {
        EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 })
    }

    fn closed(terminal: TerminalState) -> EventPayload {
        EventPayload::RunFinished(RunFinishedPayload {
            terminal_state: terminal,
            metrics: RunMetrics {
                cptv: None,
                tokens: TokenUsage::default(),
            },
        })
    }

    fn bore(child: &RunId) -> EventPayload {
        EventPayload::ChildRunCreated(ChildRunCreatedPayload {
            child_run_id: child.clone(),
            child_workflow_hash: ContentHash::sha256(b"child"),
        })
    }

    #[test]
    fn a_run_still_moving_has_no_successor() {
        let log = [event(1, started())];
        assert!(!succeeds(&log, &OTHER));
    }

    #[test]
    fn a_run_that_closed_any_other_way_has_no_successor() {
        for terminal in [
            TerminalState::Done,
            TerminalState::Failed,
            TerminalState::Cancelled,
        ] {
            let log = [event(1, started()), event(2, closed(terminal))];
            assert!(!succeeds(&log, &OTHER), "{terminal:?}");
        }
    }

    #[test]
    fn the_run_appending_after_a_promoted_close_is_the_successor() {
        let log = [
            event(1, started()),
            event(2, closed(TerminalState::Promoted)),
        ];
        assert!(succeeds(&log, &OTHER));
    }

    #[test]
    fn a_child_this_run_bore_is_never_mistaken_for_its_successor() {
        let log = [
            event(1, started()),
            event(2, bore(&OTHER)),
            event(3, closed(TerminalState::Promoted)),
        ];
        assert!(!succeeds(&log, &OTHER));
    }
}
