//! See [`super`]. The page a run's stage shows: the run's, drawn at the run as kept, or
//! its picked node's; and when it is read again.
//!
//! The run's page stands at each draw at the run in view's frame at the instant and its
//! state, so its headline, its furrow and its gist move with the run. What else the page
//! says is read with it, what the run waits on among it, so the page shown is read again
//! when the run turns: where the run's phase, its pause or the run it is, a successor in
//! its place, differs from those it was last asked at. The page asked as the stage
//! opens, before the run is kept, stands for the run as it is first kept.

use yunta_api::FollowedRun;
use yunta_core::{RunId, Seq};
use yunta_render::page::Run;
use yunta_replay::frame::RunPhase;

use super::{Framed, RunStage};
use crate::stage::Want;
use crate::{FollowedRuns, Operation};

/// Where a run stands turned: the run, its phase and the pause it stands at. Not its
/// frame, which moves with the clock.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Turn {
    run: RunId,
    phase: RunPhase,
    pause: Option<Seq>,
}

impl Turn {
    /// Where `run` stands turned at its head: none for a run with no event. Where a run
    /// stands reads no clock, so its frame is taken at the instant of its last event.
    fn of(run: &FollowedRun) -> Option<Turn> {
        let at = run.state.clock().last_at()?;
        Some(Turn {
            run: run.run.clone(),
            phase: run.frame(at).phase,
            pause: run.state.run.paused().map(|(_, at)| at),
        })
    }
}

impl RunStage {
    /// Where the run in view `followed` keeps stands turned, where it is this stage's run.
    pub(super) fn kept_turn(&self, followed: &FollowedRuns) -> Option<Turn> {
        let in_view = followed.in_view.as_deref()?;
        (in_view.run.run == self.run)
            .then(|| Turn::of(&in_view.run))
            .flatten()
    }

    /// What the stage asks once the state folded a piece of what it follows into
    /// `followed`: the page it shows read again, where the run turned since that page was
    /// last asked; nothing the first time the run is kept, which the page asked as the
    /// stage opened stands for.
    pub(crate) fn followed_on(&mut self, followed: &FollowedRuns) -> Vec<Want> {
        let Some(turn) = self.kept_turn(followed) else {
            return Vec::new();
        };
        if self.turned.as_ref() == Some(&turn) {
            return Vec::new();
        }
        match self.turned.replace(turn) {
            Some(_) => vec![Want::Work(self.page_shown())],
            None => Vec::new(),
        }
    }

    /// The reading of the page the stage shows: its picked node's, or the run's.
    pub(super) fn page_shown(&self) -> Operation {
        let run = self.run.clone();
        match &self.picked {
            Some((node, _)) => Operation::ReadNodePage {
                run,
                node: node.clone(),
            },
            None => Operation::ReadPage { run },
        }
    }
}

impl Framed<'_> {
    /// `page` standing at the run framed: the kept frame and state in place of those it
    /// was read with. None for a page of another run, read before the stage moved onto
    /// this one.
    pub(super) fn page(&self, page: &Run) -> Option<Run> {
        (page.run_id == self.in_view.run.run).then(|| Run {
            scope: page.scope,
            depth: page.depth,
            run_id: page.run_id.clone(),
            frame: self.frame.clone(),
            state: self.in_view.run.state.clone(),
            places: page.places.clone(),
            decision: page.decision.clone(),
            plans: page.plans.clone(),
        })
    }
}
