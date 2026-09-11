//! The events a surface has of one run, folded so the same event
//! arriving twice, late, or not at all cannot change what is drawn.

use std::collections::BTreeMap;

use yunta_core::events::StoredEvent;
use yunta_core::Seq;

/// One run's log as the surface holds it: the prefix it can draw from,
/// and whatever arrived above a hole in it.
///
/// A run's seqs count up from [`Seq::FIRST`] without gaps, so a prefix
/// that is contiguous from the first event is a complete reading of the
/// run up to that point — which is what every derivation expects. Events
/// past a hole are kept, not drawn: replay of a log missing an event in
/// the middle is a reading of a log nobody wrote.
///
/// Arrival order is not the log's order. Parallel nodes emit from
/// different tasks and storage assigns the seq under its own lock, so a
/// later seq routinely arrives first; that is why a hole is only ever
/// provisional, and why [`Folded::gap`] is a question and not a verdict.
#[derive(Default)]
pub(super) struct Folded {
    settled: Vec<StoredEvent>,
    above: BTreeMap<Seq, StoredEvent>,
}

impl Folded {
    /// Takes one event, ignoring any at or below the seq already folded:
    /// the same event delivered twice, and an event a re-read of the log
    /// already supplied, both land here and change nothing.
    pub(super) fn fold(&mut self, event: StoredEvent) {
        if event.seq < self.expected() {
            return;
        }
        self.above.insert(event.seq, event);
        self.settle();
    }

    /// Takes the log as storage holds it, which is authoritative: every
    /// event it carries is settled, and only what arrived past its end
    /// stays waiting.
    pub(super) fn refill(&mut self, events: Vec<StoredEvent>) {
        if events.len() <= self.settled.len() {
            return;
        }
        self.settled = events;
        let expected = self.expected();
        self.above.retain(|seq, _| *seq >= expected);
        self.settle();
    }

    /// The contiguous prefix, in log order — what a frame derives from.
    pub(super) fn settled(&self) -> &[StoredEvent] {
        &self.settled
    }

    /// Whether an event is being held above a hole. A hole that survives
    /// a redraw is a dropped frame, and the log is where it is recovered.
    pub(super) fn gap(&self) -> bool {
        !self.above.is_empty()
    }

    /// The position the prefix continues at.
    fn expected(&self) -> Seq {
        self.settled
            .last()
            .map_or(Seq::FIRST, |last| last.seq.next())
    }

    /// Moves everything the prefix can now reach out of the waiting room.
    fn settle(&mut self) {
        while let Some(event) = self.above.remove(&self.expected()) {
            self.settled.push(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::events::{EventBody, EventPayload, NodeStartedPayload};
    use yunta_core::RunId;
    use yunta_testkit::FixedClock;

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");

    fn event(seq: u64) -> StoredEvent {
        StoredEvent {
            run_id: RUN.clone(),
            seq: Seq::try_from(seq as i64).expect("a positive seq"),
            timestamp: yunta_core::Clock::now(&FixedClock),
            node_id: None,
            body: EventBody::Known(EventPayload::NodeStarted(NodeStartedPayload {
                attempt: seq as u32,
            })),
        }
    }

    /// Every event of these tests is stamped by the same frozen clock:
    /// nothing here reads a wall clock, so nothing here can race one.
    fn seqs(folded: &Folded) -> Vec<u64> {
        folded.settled().iter().map(|e| e.seq.get()).collect()
    }

    #[test]
    fn an_event_at_or_below_what_is_folded_changes_nothing() {
        let mut folded = Folded::default();
        folded.fold(event(1));
        folded.fold(event(2));
        folded.fold(event(1));
        folded.fold(event(2));
        assert_eq!(seqs(&folded), vec![1, 2]);
    }

    #[test]
    fn events_that_arrive_out_of_order_are_drawn_in_the_logs_order() {
        let mut folded = Folded::default();
        folded.fold(event(3));
        folded.fold(event(1));
        assert_eq!(seqs(&folded), vec![1], "3 waits on the hole at 2");
        assert!(folded.gap());
        folded.fold(event(2));
        assert_eq!(seqs(&folded), vec![1, 2, 3]);
        assert!(!folded.gap());
    }

    #[test]
    fn a_reading_of_the_log_closes_a_hole_a_dropped_frame_left() {
        let mut folded = Folded::default();
        folded.fold(event(1));
        folded.fold(event(4));
        assert!(folded.gap());
        folded.refill(vec![event(1), event(2), event(3)]);
        assert_eq!(seqs(&folded), vec![1, 2, 3, 4]);
        assert!(!folded.gap());
    }

    #[test]
    fn a_reading_that_knows_less_than_the_surface_is_left_alone() {
        let mut folded = Folded::default();
        folded.fold(event(1));
        folded.fold(event(2));
        folded.refill(vec![event(1)]);
        assert_eq!(seqs(&folded), vec![1, 2]);
    }
}
