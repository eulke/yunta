//! See [`super`]. What a stage wants of the state after a stroke, a piece of news or an
//! answer, which the state carries out.

use super::Stage;
use crate::dialog::Confirmation;
use crate::{Address, Operation, Ticket};
use yunta_core::step::{NextStep, Reach};
use yunta_core::RunId;
use yunta_render::screen::Confirming;

/// What a stage wants of the state after a stroke, a piece of news or an answer.
pub(crate) enum Want {
    /// Work Yunta does.
    Work(Operation),
    /// Work the stage asked for and wants no more: stopped where it is under way, and
    /// what it would answer dropped.
    LetGo(Operation),
    /// What the performer holds under a ticket for the stage, no longer wanted:
    /// dropped.
    Release(Ticket),
    /// A page or a link a browser reads, shown: by a terminal through the system's
    /// opener, by the desktop in a window.
    Show(Address),
    /// An act confirmed before it is done.
    Confirm(Confirmation),
    /// A stage opened over this one, which the person comes back from.
    Open(Stage),
    /// A stage in this one's place: what this one was opened over stays under it.
    Replace(Stage),
    /// Back to the stage this one was opened over.
    Back,
    /// Back to the run's stage a page over it was opened over, and on to the run's own
    /// decision as `a` there brings it: the lead given back to the run, its prompt in place.
    /// Where `a` takes the person from a page over a run whose attention row names the run's
    /// own decision.
    BackToItsDecision,
    /// Down to the nearest field under this one, noting there what leaving said, where it
    /// said anything (what leaving a run says of it); where no field stands under it, down to
    /// the stage at the bottom of what stands. No stage is built, and the one at the bottom is
    /// never dropped.
    ToTheField(Option<String>),
    /// The person leaves the application.
    Leave,
}

impl Want {
    /// What taking `step` wants, the one mapping from a step to it, by a key or from the
    /// palette: this repository's runs, down to the nearest field under the stage; stopping a run and closing it, once confirmed; going on with it, at once;
    /// any other, the stage it opens ([`Stage::opened_by`]), a new run among them, opened over
    /// this one; nothing where no stage takes it.
    pub(crate) fn taking(step: &NextStep) -> Vec<Want> {
        match step {
            NextStep::ListRuns {
                reach: Reach::Repository,
            } => vec![Want::ToTheField(None)],
            NextStep::Cancel { run } => {
                let asks = Confirming::Stop {
                    run: run.handle().to_string(),
                };
                let same = vec![NextStep::Cancel { run: run.clone() }];
                let cancel = Operation::Cancel { run: run.clone() };
                vec![Want::Confirm(Confirmation::new(asks, same, cancel))]
            }
            NextStep::Resume { run } => vec![Want::Work(Operation::Resume { run: run.clone() })],
            NextStep::Close { run } => {
                let asks = Confirming::Close {
                    runs: vec![run.handle().to_string()],
                    left: Vec::new(),
                };
                let same = vec![NextStep::Close { run: run.clone() }];
                let close = Operation::Close {
                    runs: vec![run.clone()],
                };
                vec![Want::Confirm(Confirmation::new(asks, same, close))]
            }
            other => Stage::opened_by(other)
                .map(Want::Open)
                .into_iter()
                .collect(),
        }
    }

    /// `run`'s screen in place of this one: where `a` takes the person from a screen whose
    /// attention row names another run.
    pub(crate) fn in_place(run: &RunId) -> Vec<Want> {
        let opened = Stage::opened_by(&NextStep::status(run));
        opened.map(Want::Replace).into_iter().collect()
    }
}
