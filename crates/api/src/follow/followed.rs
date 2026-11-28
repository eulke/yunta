//! See [`super`]. A run as its follower knows it: the manifest it froze, the state its
//! events derive up to its head, its engine as last read, the frame those derive, the
//! decision it waits on there and what a reader can do next for it. It holds no event: it is
//! owned and `Clone`, so an interface keeps one beside what it draws and folds into it each
//! piece of news a [`Following`](super::Following) hands back, by the fold the following
//! itself makes, taking the moments of each batch from it as it folds. The surface of a run attached to
//! its terminal keeps one in the process that drives the run, and folds into it what that
//! run's engine hands it, batched as a look batches it.
//!
//! The frame is derived once for each head and each reading of the engine, where either
//! moves, and kept with what it was derived from: what the frame reads is the run's id,
//! its manifest's workflow, its state and its engine, and none of them changes but through
//! the fold that derives the frame again, so no frame stands for a head the run has left.
//! A draw takes the frame kept and works out only what the clock adds to it
//! ([`RunTimes`]), at its own instant. Not a frame kept for an instant: nothing kept moves
//! with the clock, so nothing kept goes stale as time passes.
//!
//! The decision the run waits on is rebuilt once for each head, as the run's log alone
//! rebuilds it (`awaited_escalation`, a walk of the state), the one owner of what a run asks:
//! what it is on, and its menu or why its gate cannot put one. Whether the run waits on a
//! decision ([`FollowedRun::menu`]), the pause the follower reads what it asks for, and what
//! a reader can do next for the run (`RunFrame::actions`) are all read from it, the actions
//! worked out each time the frame is derived: a draw reads them and works out none, so a
//! field of many runs costs no walk of any run's state as it is drawn.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use yunta_core::events::{AwaitedDecision, StoredEvent};
use yunta_core::refusal::CloseRefusal;
use yunta_core::{Manifest, RunId};
use yunta_render::page::FieldRow;
use yunta_replay::frame::{Action, Moment, RunFrame, RunPhase, RunTimes};
use yunta_replay::registry::Engine;
use yunta_replay::RunState;

use super::RunNews;

/// A run as its follower knows it. Not an [`OpenedRun`](crate::OpenedRun), which reads
/// a run's log once for a page.
#[derive(Debug, Clone)]
pub struct FollowedRun {
    run: RunId,
    manifest: Manifest,
    state: RunState,
    engine: Engine,
    /// The frame the four above derive, derived where any of them moves: shared with the
    /// field's rows that read it.
    frame: Arc<RunFrame>,
    /// The decision the run waits on at its head, as its log alone rebuilds it: rebuilt once
    /// for each head, never at a draw; none where it waits on none.
    decision: Option<Arc<AwaitedDecision>>,
    /// What a reader can do next for the run, its frame's actions chosen by its decision:
    /// worked out where the frame is derived, never at a draw.
    actions: Vec<Action>,
    /// How many times its frame was derived, read by a test that holds a draw derives none.
    #[cfg(debug_assertions)]
    derivations: u64,
    /// How many times its acts were worked out, read by a test that holds a draw works none
    /// out.
    #[cfg(debug_assertions)]
    acts_worked_out: u64,
}

impl FollowedRun {
    /// `run` as `events` derive it, with the manifest it froze and its engine as its
    /// registry reads: its frame derived, the decision it waits on at its head rebuilt, and
    /// what a reader can do next for it worked out.
    pub fn read(run: RunId, manifest: Manifest, events: &[StoredEvent], engine: Engine) -> Self {
        let state = RunState::replay(events);
        let frame = Arc::new(RunFrame::of(
            &run,
            &manifest.workflow,
            &state,
            None,
            &engine,
        ));
        let mut read = FollowedRun {
            run,
            manifest,
            state,
            engine,
            frame,
            decision: None,
            actions: Vec::new(),
            #[cfg(debug_assertions)]
            derivations: 1,
            #[cfg(debug_assertions)]
            acts_worked_out: 0,
        };
        read.decided();
        read.acted();
        read
    }

    /// The run's id.
    pub fn run(&self) -> &RunId {
        &self.run
    }

    /// The manifest the run froze.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The state its events derive, up to its head.
    pub fn state(&self) -> &RunState {
        &self.state
    }

    /// The process driving the run, as its registry said when last read.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The run's frame at its head, beside its engine as last read: derived once for both,
    /// and the same at every instant until either moves.
    pub fn frame(&self) -> &RunFrame {
        &self.frame
    }

    /// What a reader can do next for the run at its head, by the word its frame reads: its
    /// frame's actions, chosen by the decision it waits on there, as kept since the frame was
    /// last derived.
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    /// The decision the run waits on at its head, as its log alone rebuilds it: what it is on,
    /// and its menu or why its gate cannot put one; none where it waits on none. Not what a
    /// person is asked, which reads the documents a gate shows.
    pub fn decision(&self) -> Option<&AwaitedDecision> {
        self.decision.as_deref()
    }

    /// Whether the run, at its head, waits on a decision a person answers, which `yunta
    /// status` says: whether it has a [`FollowedRun::decision`], its menu put or not.
    pub fn menu(&self) -> bool {
        self.decision.is_some()
    }

    /// Why a close refuses the run at its head, by the engine's own rule, from its state and
    /// its engine as kept: none where a close takes it. Not one of [`FollowedRun::actions`],
    /// which say what a reader is offered: read where a run named for a close is left open.
    pub fn close_refused(&self) -> Option<CloseRefusal> {
        yunta_engine::close_refusal(&self.state, &self.engine)
    }

    /// What the clock adds to the run's frame at `now`, worked out from the state the frame
    /// was derived from.
    pub fn times(&self, now: DateTime<Utc>) -> RunTimes {
        RunTimes::of(&self.state, &self.engine, now)
    }

    /// How many times its frame was derived: once read, and once more for each piece of
    /// news that moved its head or its engine.
    #[cfg(debug_assertions)]
    pub fn derivations(&self) -> u64 {
        self.derivations
    }

    /// How many times its acts were worked out: once read, and once more each time its frame
    /// was derived again.
    #[cfg(debug_assertions)]
    pub fn acts_worked_out(&self) -> u64 {
        self.acts_worked_out
    }

    /// `news` folded in, and the moments its events are: the events that came, each by
    /// [`RunState::chronicled`], the frame derived and the menu worked out at the new head;
    /// the engine as its registry reads now, the frame derived again where it reads
    /// otherwise than before; or the successor a promotion handed the work to, in this
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

    /// The run's row in a field at `now`: its frame, what the clock adds to it there, and
    /// whether it waits on a decision at its head. The one way a run's row is made, for a listing
    /// and a following alike.
    pub fn row(&self, now: DateTime<Utc>) -> FieldRow {
        FieldRow {
            frame: Arc::clone(&self.frame),
            times: self.times(now),
            menu: self.menu(),
        }
    }

    /// `news` folded in, each of its events taken into the state by `take`.
    fn taking(&mut self, news: &RunNews, mut take: impl FnMut(&mut RunState, &StoredEvent)) {
        match news {
            RunNews::Events(events) => {
                let head = self.state.head();
                events.iter().for_each(|event| take(&mut self.state, event));
                if self.state.head() != head {
                    self.framed();
                    self.decided();
                    self.acted();
                }
            }
            RunNews::Engine(engine) => {
                if *engine != self.engine {
                    self.engine = engine.clone();
                    self.framed();
                    self.acted();
                }
            }
            RunNews::Successor { run, .. } => *self = (**run).clone(),
            RunNews::Said { .. }
            | RunNews::NoSuccessor
            | RunNews::Awaited(_)
            | RunNews::AwaitedUnread { .. }
            | RunNews::Document { .. }
            | RunNews::Remark(_) => {}
        }
    }

    /// The frame derived again from what it reads, which moved.
    fn framed(&mut self) {
        self.frame = Arc::new(RunFrame::of(
            &self.run,
            &self.manifest.workflow,
            &self.state,
            None,
            &self.engine,
        ));
        #[cfg(debug_assertions)]
        {
            self.derivations += 1;
        }
    }

    /// The decision the run waits on at its head rebuilt: none for a run whose log holds no
    /// event, nor for one that waits on nothing.
    fn decided(&mut self) {
        let waits = matches!(self.frame.phase, RunPhase::Waiting { .. });
        let read = self.state.clock().last_at().is_some();
        let decision = (waits && read)
            .then(|| yunta_engine::awaited_escalation(&self.manifest, &self.state))
            .flatten();
        self.decision = decision.map(Arc::new);
    }

    /// What a reader can do next for the run worked out from its frame and its decision's
    /// menu, where one was put.
    fn acted(&mut self) {
        let menu = self.decision.as_deref().and_then(AwaitedDecision::menu);
        self.actions = self.frame.actions(menu.map(|escalation| &**escalation));
        #[cfg(debug_assertions)]
        {
            self.acts_worked_out += 1;
        }
    }
}
