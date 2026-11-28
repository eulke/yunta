//! See [`super`]. Where `a` takes the person from a page over a run whose attention row
//! names the run's own decision: back to the run's stage, which then gives the run the lead
//! back, its prompt in place as `a` there brings it.

use super::State;
use crate::{Request, Stage};

impl State {
    /// Back to the stage the one the person is at was opened over, as `State::back` goes,
    /// and, where it is a run's, on to the run's own decision as `a` there brings it.
    pub(super) fn back_to_its_decision(&mut self) -> Vec<Request> {
        let mut asked = self.back();
        let by = self.on_screen();
        let wants = match self.stage.as_mut().map(|standing| &mut standing.stage) {
            Some(Stage::Run(run)) => run.attending_its_decision(&self.followed_runs, self.now),
            Some(_) | None => Vec::new(),
        };
        asked.extend(self.wanted(by, wants));
        asked
    }
}
