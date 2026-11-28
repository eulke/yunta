//! The run a surface draws, kept as its follower keeps it, and the events
//! that came above a hole in its log, folded so the same event arriving
//! twice, late, or not at all cannot change what is drawn.
//!
//! The run is held by the one owner every interface keeps a followed run
//! in, [`FollowedRun`]: each event is folded once, as the head reaches it;
//! the frame is derived once for each piece of news that moves the head or
//! the reading of the engine; the moments of each piece are handed back as
//! it is folded. So a redraw reads no event and derives no frame, and works
//! out only what the clock adds. Not a copy of the log replayed at each
//! redraw, which reads the whole run on every beat and every event in the
//! process that drives it. The events the surface still holds are only the
//! ones waiting above a hole.

use std::collections::BTreeMap;

use yunta_api::{FollowedRun, RunNews};
use yunta_core::events::StoredEvent;
use yunta_core::{Manifest, RunId, Seq};
use yunta_replay::frame::Moment;
use yunta_replay::registry::Engine;

/// One run as the surface holds it: folded as far as its log reaches with
/// no hole, and whatever arrived above a hole in it.
///
/// A run's seqs count up from [`Seq::FIRST`] without gaps, so a prefix
/// that is contiguous from the first event is a complete reading of the
/// run up to that point — which is what every derivation expects. Events
/// past a hole are kept, not folded: a fold of a log missing an event in
/// the middle is a reading of a log nobody wrote.
///
/// Arrival order is not the log's order. Parallel nodes emit from
/// different tasks and storage assigns the seq under its own lock, so a
/// later seq routinely arrives first; that is why a hole is only ever
/// provisional, and why [`Folded::gap`] is a question and not a verdict.
pub(super) struct Folded {
    run: FollowedRun,
    above: BTreeMap<Seq, StoredEvent>,
    /// How many events it handed to the run's fold since it was read: what
    /// a test counts to hold that a redraw reads none.
    #[cfg(test)]
    events_read: usize,
}

impl Folded {
    /// `run`, frozen as `manifest` says and driven by the process `engine`
    /// reads, folded as far as `log` reaches with no hole. `log` is the run's
    /// log as the surface opened with it: what it holds is no news, so it
    /// earns no moment.
    pub(super) fn read(
        run: RunId,
        manifest: Manifest,
        engine: Engine,
        log: Vec<StoredEvent>,
    ) -> Self {
        let mut above = BTreeMap::new();
        wait(&mut above, &run, Seq::FIRST, log);
        let reached = reach(&mut above, Seq::FIRST);
        Folded {
            #[cfg(test)]
            events_read: reached.len(),
            run: FollowedRun::read(run, manifest, &reached, engine),
            above,
        }
    }

    /// Takes `events`, and hands back the moment of each one the run's head
    /// reaches now, in the log's order. An event of another run, or at or
    /// below the head, changes nothing: the same event delivered twice, and
    /// an event a read of the log already supplied, both land here.
    pub(super) fn fold(&mut self, events: impl IntoIterator<Item = StoredEvent>) -> Vec<Moment> {
        let expected = self.expected();
        wait(&mut self.above, self.run.run(), expected, events);
        let reached = reach(&mut self.above, expected);
        if reached.is_empty() {
            return Vec::new();
        }
        #[cfg(test)]
        {
            self.events_read += reached.len();
        }
        self.run.took(&RunNews::Events(reached))
    }

    /// Takes the process driving the run as `engine` reads it now: the frame
    /// is derived again only where that reading moved. It earns no moment.
    pub(super) fn engine(&mut self, engine: Engine) {
        self.run.took(&RunNews::Engine(engine));
    }

    /// The run as far as its log reaches with no hole.
    pub(super) fn run(&self) -> &FollowedRun {
        &self.run
    }

    /// The last event folded; none before the first.
    pub(super) fn head(&self) -> Option<Seq> {
        self.run.state().head()
    }

    /// Whether an event is being held above a hole. A hole that survives
    /// a redraw is a dropped frame, and the log is where it is recovered.
    pub(super) fn gap(&self) -> bool {
        !self.above.is_empty()
    }

    /// How many events it handed to the run's fold since it was read.
    #[cfg(test)]
    pub(super) fn events_read(&self) -> usize {
        self.events_read
    }

    /// The position the run continues at.
    fn expected(&self) -> Seq {
        self.head().map_or(Seq::FIRST, Seq::next)
    }
}

/// Puts each of `events` that is `run`'s, at or past `expected`, in the
/// waiting room `above`: one event a seq, so the same one twice is kept once.
fn wait(
    above: &mut BTreeMap<Seq, StoredEvent>,
    run: &RunId,
    expected: Seq,
    events: impl IntoIterator<Item = StoredEvent>,
) {
    for event in events {
        if &event.run_id == run && event.seq >= expected {
            above.insert(event.seq, event);
        }
    }
}

/// Takes out of the waiting room `above`, in the log's order, every event
/// the run reaches from `expected` on, up to the first hole.
fn reach(above: &mut BTreeMap<Seq, StoredEvent>, mut expected: Seq) -> Vec<StoredEvent> {
    let mut reached = Vec::new();
    while let Some(event) = above.remove(&expected) {
        expected = expected.next();
        reached.push(event);
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::events::{EventPayload, NodeEvent, NodeStartedPayload};
    use yunta_core::Pid;
    use yunta_testkit_core::Log;

    const RUN: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P5";

    /// A run's log of `len` events, every one stamped by the same frozen
    /// clock: nothing here reads a wall clock, so nothing here can race
    /// one.
    fn log(len: u32) -> Vec<StoredEvent> {
        (1..=len)
            .fold(Log::for_run(RUN), |log, attempt| {
                log.event(EventPayload::Node(NodeEvent::Started(
                    NodeStartedPayload::attempt(attempt),
                )))
            })
            .build()
    }

    /// The run the log above is of, read with none of its events.
    fn unread() -> Folded {
        let manifest = crate::surface::frozen("name: folded\nnodes: []\n");
        Folded::read(
            RunId::from(RUN),
            manifest,
            Engine::this(Pid::current()),
            Vec::new(),
        )
    }

    /// The seq of each moment, in the order they were handed back.
    fn seqs(moments: &[Moment]) -> Vec<u64> {
        moments.iter().map(|moment| moment.seq.get()).collect()
    }

    #[test]
    fn an_event_at_or_below_what_is_folded_changes_nothing() {
        let log = log(2);
        let mut folded = unread();
        assert_eq!(seqs(&folded.fold(log.clone())), vec![1, 2]);
        assert_eq!(seqs(&folded.fold(log)), Vec::<u64>::new());
        assert_eq!(folded.head(), Some(Seq::from(2)));
    }

    #[test]
    fn events_that_arrive_out_of_order_are_drawn_in_the_logs_order() {
        let log = log(3);
        let mut folded = unread();
        assert_eq!(seqs(&folded.fold([log[2].clone()])), Vec::<u64>::new());
        assert_eq!(
            seqs(&folded.fold([log[0].clone()])),
            vec![1],
            "3 waits on the hole at 2"
        );
        assert!(folded.gap());
        assert_eq!(seqs(&folded.fold([log[1].clone()])), vec![2, 3]);
        assert!(!folded.gap());
    }

    #[test]
    fn a_reading_of_the_log_closes_a_hole_a_dropped_frame_left() {
        let log = log(4);
        let mut folded = unread();
        folded.fold([log[0].clone(), log[3].clone()]);
        assert!(folded.gap());
        assert_eq!(seqs(&folded.fold(log[1..3].to_vec())), vec![2, 3, 4]);
        assert!(!folded.gap());
    }

    #[test]
    fn a_reading_that_knows_less_than_the_surface_is_left_alone() {
        let log = log(2);
        let mut folded = unread();
        folded.fold(log.clone());
        assert_eq!(seqs(&folded.fold(log[..1].to_vec())), Vec::<u64>::new());
        assert_eq!(folded.head(), Some(Seq::from(2)));
    }

    #[test]
    fn the_log_a_surface_opens_with_earns_no_moment_and_the_next_event_does() {
        let log = log(3);
        let manifest = crate::surface::frozen("name: folded\nnodes: []\n");
        let mut folded = Folded::read(
            RunId::from(RUN),
            manifest,
            Engine::this(Pid::current()),
            log[..2].to_vec(),
        );
        assert_eq!(folded.head(), Some(Seq::from(2)));
        assert_eq!(seqs(&folded.fold(log)), vec![3]);
    }
}
