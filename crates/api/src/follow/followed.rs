//! See [`super`]. A run as its follower knows it: the manifest it froze, the state its
//! events derive up to its head, its engine as last read and whether it offers a menu
//! there. It holds no event: it is owned and `Clone`, so an interface keeps one beside
//! what it draws and folds into it each piece of news a
//! [`Following`](super::Following) hands back, by the fold the following itself makes,
//! taking the moments of each batch from it as it folds.

use chrono::{DateTime, Utc};
use yunta_core::events::StoredEvent;
use yunta_core::{Manifest, RunId};
use yunta_render::page::FieldRow;
use yunta_replay::frame::{Moment, RunFrame};
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
    /// The state its events derive, up to its head.
    pub state: RunState,
    /// The process driving the run, as its registry said when last read.
    pub engine: Engine,
    /// Whether the run, at its head, stopped on a decision with a menu, which `yunta
    /// status` lists the options of: worked out once for each head, never at a draw.
    pub menu: bool,
}

impl FollowedRun {
    /// `run` as `events` derive it, with the manifest it froze and its engine as its
    /// registry reads.
    pub(crate) fn read(
        run: RunId,
        manifest: Manifest,
        events: &[StoredEvent],
        engine: Engine,
    ) -> Self {
        let mut read = FollowedRun {
            run,
            manifest,
            state: RunState::replay(events),
            engine,
            menu: false,
        };
        read.menu = read.menu_at_head();
        read
    }

    /// `news` folded in, and the moments its events are: the events that came, each by
    /// [`RunState::chronicled`], the menu worked out at the new head; the engine as its
    /// registry reads now; or the successor a promotion handed the work to, in this
    /// one's place. An event at or below the head moves nothing and is no moment. A
    /// successor hands back no moment: whoever keeps the moments takes those of the
    /// events it was read whole with, as of a run read whole. What a session said, what
    /// the run waits on a person for, what its nodes handed over and what reading it
    /// remarks on change nothing of it.
    pub fn took(&mut self, news: &RunNews) -> Vec<Moment> {
        let mut moments = Vec::new();
        self.taking(news, |state, event| moments.extend(state.chronicled(event)));
        moments
    }

    /// `news` folded in as [`FollowedRun::took`] folds it, with no moment: the copy the
    /// following keeps for itself, which nobody tells the run from.
    pub(super) fn folded(&mut self, news: &RunNews) {
        self.taking(news, RunState::fold);
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

    /// The run's row in a field at `now`: its frame there, and whether it offers a menu
    /// at its head. The one way a run's row is made, for a listing and a following alike.
    pub fn row(&self, now: DateTime<Utc>) -> FieldRow {
        FieldRow {
            frame: self.frame(now),
            menu: self.menu,
        }
    }

    /// `news` folded in, each of its events taken into the state by `take`.
    fn taking(&mut self, news: &RunNews, mut take: impl FnMut(&mut RunState, &StoredEvent)) {
        match news {
            RunNews::Events(events) => {
                let head = self.state.head();
                events.iter().for_each(|event| take(&mut self.state, event));
                if self.state.head() != head {
                    self.menu = self.menu_at_head();
                }
            }
            RunNews::Engine(engine) => self.engine = engine.clone(),
            RunNews::Successor { run, .. } => *self = (**run).clone(),
            RunNews::Said { .. }
            | RunNews::NoSuccessor
            | RunNews::Awaited(_)
            | RunNews::AwaitedUnread { .. }
            | RunNews::Document { .. }
            | RunNews::Remark(_) => {}
        }
    }

    /// Whether the run, at its head, stopped on a decision with a menu. Where a run
    /// stands reads no clock, so its frame is taken at the instant of its last event.
    fn menu_at_head(&self) -> bool {
        let Some(at) = self.state.clock().last_at() else {
            return false;
        };
        crate::offers_a_menu(&self.frame(at), &self.manifest, &self.state)
    }
}
