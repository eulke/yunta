//! See [`super`]. A run as its follower knows it: the manifest it froze, every event
//! read, the state they derive and its engine as last read. It is owned and `Clone`, so
//! an interface keeps one beside what it draws and folds into it each piece of news a
//! [`Following`](super::Following) hands back, by the fold the following itself makes.

use chrono::{DateTime, Utc};
use yunta_core::events::StoredEvent;
use yunta_core::{Manifest, RunId, Seq};
use yunta_replay::frame::RunFrame;
use yunta_replay::registry::Engine;
use yunta_replay::RunState;

use super::RunNews;

/// A run as its follower knows it. Not an [`OpenedRun`](crate::OpenedRun), which reads
/// a run's log once for a page.
#[derive(Debug, Clone)]
pub struct FollowedRun {
    pub run: RunId,
    /// The manifest the run froze.
    pub manifest: Manifest,
    /// Every event read, in the log's order, so a log that stops making sense is
    /// derived whole, which is what says where it broke.
    pub events: Vec<StoredEvent>,
    /// The state the events derive.
    pub state: RunState,
    /// The process driving the run, as its registry said when last read.
    pub engine: Engine,
}

impl FollowedRun {
    /// `run` as `events` derive it, with the manifest it froze and its engine as its
    /// registry reads.
    pub(crate) fn read(
        run: RunId,
        manifest: Manifest,
        events: Vec<StoredEvent>,
        engine: Engine,
    ) -> Self {
        let state = RunState::replay(&events);
        FollowedRun {
            run,
            manifest,
            events,
            state,
            engine,
        }
    }

    /// `news` folded in: the events that came, the engine as its registry reads now, or
    /// the run a promotion handed the work to, in this one's place. What a session said
    /// and where the run ends change nothing of it.
    pub fn took(&mut self, news: &RunNews) {
        match news {
            RunNews::Events(events) => events.iter().for_each(|event| self.take(event)),
            RunNews::Engine(engine) => self.engine = engine.clone(),
            RunNews::Successor(successor) => *self = (**successor).clone(),
            RunNews::Said(_) | RunNews::Stopped | RunNews::Stalled => {}
        }
    }

    /// The run's frame at `now`.
    pub fn frame(&self, now: DateTime<Utc>) -> RunFrame {
        RunFrame::of(
            &self.run,
            &self.manifest.workflow,
            &self.state,
            None,
            &self.engine,
            now,
        )
    }

    /// Whether the run, framed as `frame`, stopped on a decision with a menu, which
    /// `yunta status` lists the options of.
    pub fn offers_a_menu(&self, frame: &RunFrame) -> bool {
        crate::offers_a_menu(frame, &self.manifest, &self.state)
    }

    /// The seq of the last event read; a run is never followed before its first event
    /// is on the log.
    pub(super) fn last(&self) -> Seq {
        self.events.last().map_or(Seq::FIRST, |event| event.seq)
    }

    /// Folds one event into the state, deriving the whole log again when it does not
    /// fold, which is what marks where the log broke.
    fn take(&mut self, event: &StoredEvent) {
        self.events.push(event.clone());
        if self.state.apply(event).is_err() {
            self.state = RunState::replay(&self.events);
        }
    }
}
