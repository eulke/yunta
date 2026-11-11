//! See [`crate`]. Where a person is: one variant per stage, each holding what its
//! stage reads and where the person stands in it, in `stage::<name>`.
//!
//! A stage says what its screen shows, what it asks for as it opens and as a person
//! comes back to it, what a stroke does there and what news and answers of its work
//! do, each as what it wants of the state ([`Want`]): work, work let go, a
//! confirmation, another stage opened over it, going back, or leaving. The state
//! carries those out. Each stage that joins adds its variant here and its module beside
//! this one.

mod decision;
mod field;
mod first_run;
mod run;
mod signpost;
mod start;

pub use decision::DecisionStage;
pub use field::FieldStage;
pub use first_run::FirstRunStage;
pub use run::RunStage;
pub use start::StartStage;

use chrono::{DateTime, Utc};
use surco::components::{PaneLaid, Screen};
use surco::look::Look;

use yunta_api::Remark;

use crate::dialog::Confirmation;
use crate::prompt::Stroke;
use crate::{News, Operation, Outcome};

/// Where a person is.
#[derive(Debug, Clone)]
pub enum Stage {
    /// Outside a repository: that yunta works in one, and the ways on from here.
    Signpost,
    /// The runs, and the acts on them.
    Field(FieldStage),
    /// A run, followed.
    Run(RunStage),
    /// The decision a run waits on, being answered.
    Decision(DecisionStage),
    /// The first run, in a repository set up with none yet.
    FirstRun(FirstRunStage),
    /// A run being started.
    Start(StartStage),
}

/// What a stage wants of the state after a stroke, a piece of news or an answer.
pub(crate) enum Want {
    /// Work Yunta does.
    Work(Operation),
    /// Work the stage asked for and wants no more: stopped where it is under way, and
    /// what it would answer dropped.
    LetGo(Operation),
    /// An act confirmed before it is done.
    Confirm(Confirmation),
    /// A stage opened over this one, which the person comes back from.
    Open(Stage),
    /// A stage in this one's place: what this one was opened over stays under it.
    Replace(Stage),
    /// Back to the stage this one was opened over.
    Back,
    /// The person leaves the application.
    Leave,
}

impl Stage {
    /// The stage as a screen at `now`.
    pub(crate) fn screen(&self, look: &Look, now: DateTime<Utc>) -> Screen {
        match self {
            Stage::Signpost => signpost::screen(look),
            Stage::Field(field) => field.screen(look),
            Stage::Run(run) => run.screen(look, now),
            Stage::Decision(decision) => decision.screen(look),
            Stage::FirstRun(first_run) => first_run.screen(look),
            Stage::Start(start) => start.screen(look),
        }
    }

    /// What the stage asks for as it opens.
    pub(crate) fn opening(&self) -> Vec<Want> {
        match self {
            Stage::Signpost | Stage::Field(_) => Vec::new(),
            Stage::Run(run) => run.opening(),
            Stage::Decision(decision) => decision.opening(),
            Stage::FirstRun(first_run) => first_run.opening(),
            Stage::Start(_) => Vec::new(),
        }
    }

    /// What the stage asks for when a person comes back to it.
    pub(crate) fn returned(&self) -> Vec<Want> {
        match self {
            Stage::Signpost | Stage::Decision(_) | Stage::Start(_) => Vec::new(),
            Stage::Field(field) => field.returned(),
            Stage::FirstRun(first_run) => first_run.returned(),
            Stage::Run(run) => run.returned(),
        }
    }

    /// What `stroke` asks for here at `now`.
    pub(crate) fn pressed(&mut self, stroke: Stroke, now: DateTime<Utc>) -> Vec<Want> {
        match self {
            Stage::Signpost => signpost::pressed(stroke),
            Stage::Field(field) => field.pressed(stroke),
            Stage::Run(run) => run.pressed(stroke, now),
            Stage::Decision(decision) => decision.pressed(stroke),
            Stage::FirstRun(first_run) => first_run.pressed(stroke),
            Stage::Start(start) => start.pressed(stroke),
        }
    }

    /// Whether a field is open here.
    pub(crate) fn typing(&self) -> bool {
        match self {
            Stage::Decision(decision) => decision.typing(),
            Stage::FirstRun(first_run) => first_run.typing(),
            Stage::Start(start) => start.typing(),
            Stage::Signpost | Stage::Field(_) | Stage::Run(_) => false,
        }
    }

    /// What a surface laid out of a pane the stage scrolls, which it reads on the next
    /// stroke.
    pub(crate) fn laid(&mut self, pane: PaneLaid) {
        match self {
            Stage::Decision(decision) => decision.laid(pane),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Run(_)
            | Stage::FirstRun(_)
            | Stage::Start(_) => {}
        }
    }

    /// What news of lasting work the stage asked for does here at `now`.
    pub(crate) fn heard(&mut self, news: News, now: DateTime<Utc>) -> Vec<Want> {
        match self {
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Decision(_)
            | Stage::FirstRun(_)
            | Stage::Start(_) => Vec::new(),
            Stage::Run(run) => run.heard(news, now),
        }
    }

    /// What an answer to work the stage asked for does here.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match self {
            Stage::Signpost => Vec::new(),
            Stage::Field(field) => field.answered(outcome),
            Stage::Run(run) => run.answered(outcome),
            Stage::Decision(decision) => decision.answered(outcome),
            Stage::FirstRun(first_run) => first_run.answered(outcome),
            Stage::Start(start) => start.answered(outcome),
        }
    }

    /// What a piece of work the stage asked for remarks while it works.
    pub(crate) fn remarked(&mut self, remark: Remark) {
        match self {
            Stage::Start(start) => start.remarked(remark),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Run(_)
            | Stage::Decision(_)
            | Stage::FirstRun(_) => {}
        }
    }
}
