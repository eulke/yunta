//! See [`crate`]. Where a person is: one variant per stage, each holding what its
//! stage reads and where the person stands in it, in `stage::<name>`.
//!
//! A stage says what its screen shows, what it asks for as it opens and as a person
//! comes back to it, what a stroke does there and what news and answers of its work
//! do, each as what it wants of the state ([`Want`]): work, work let go, a
//! confirmation, another stage opened over it, going back, or leaving. The state
//! carries those out; an act a confirmation stood before is handed back to the stage
//! that asked for it once confirmed ([`Stage::confirmed`]), which asks for its work.
//!
//! A stage says what it offers as [`Act`]s, each a step as a command line names it and
//! the key that takes it there, which the palette lists over it before what every stage
//! offers ([`Stage::always`]), each of those opening the stage that takes it. A stage whose work found something the performer holds for later
//! work says the ticket it is held under ([`Stage::held`]), which the state lets go
//! when the stage is left. Each stage that joins adds its variant here and its module
//! beside this one.

mod catalog;
mod decision;
mod doctor;
mod evidence;
mod field;
mod first_run;
mod graph;
mod run;
mod schema;
mod setup;
mod signpost;
mod start;
mod stats;
mod steps;
mod upkeep;

pub use catalog::CatalogStage;
pub use decision::DecisionStage;
pub use doctor::DoctorStage;
pub use evidence::{Evidence, EvidenceStage};
pub use field::FieldStage;
pub use first_run::FirstRunStage;
pub use graph::GraphStage;
pub use run::RunStage;
pub use schema::SchemaStage;
pub use setup::SetupStage;
pub use start::StartStage;
pub use stats::StatsStage;
pub use upkeep::UpkeepStage;

use chrono::{DateTime, Utc};
use surco::components::{Screen, ScreenLaid};
use surco::look::Look;

use yunta_api::{FieldReach, Remark};
use yunta_core::step::NextStep;
use yunta_core::RunId;

use crate::dialog::Confirmation;
use crate::prompt::Stroke;
use crate::{FollowedRuns, News, Operation, Outcome, Ticket};

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
    /// A repository Yunta was never set up in, being set up.
    Setup(SetupStage),
    /// The workflows the repository can run, one pointed at.
    Catalog(CatalogStage),
    /// What a run leaves: its receipt, its verification, its page for a browser.
    Evidence(EvidenceStage),
    /// What a run cost, or what a workflow's past runs did.
    Stats(StatsStage),
    /// A workflow's graph, as written or as a run left it.
    Graph(GraphStage),
    /// What doctor checks of this machine and this project.
    Doctor(DoctorStage),
    /// What collecting what runs left would reclaim, then what it reclaimed.
    Upkeep(UpkeepStage),
    /// The shapes of the documents Yunta reads.
    Schema(SchemaStage),
}

/// An act a stage offers: the step it takes, as a command line names it, and the key
/// that takes it there. Not the step alone: the palette takes an act by its key, so a
/// row chosen there does what pressing the key does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Act {
    pub(crate) step: NextStep,
    pub(crate) key: Stroke,
}

impl Act {
    /// `step`, taken here by `key`.
    pub(crate) fn by(key: Stroke, step: NextStep) -> Self {
        Act { step, key }
    }
}

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
    /// A page a browser reads, shown: by a terminal through the system's opener,
    /// by the desktop in a window.
    Show(std::path::PathBuf),
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

impl Want {
    /// What taking `step` wants: the stage it opens ([`Stage::opened_by`]), opened over
    /// this one; nothing where no stage takes it.
    pub(crate) fn taking(step: &NextStep) -> Vec<Want> {
        Stage::opened_by(step).map(Want::Open).into_iter().collect()
    }
}

impl Stage {
    /// The run this stage shows, which the state follows under every stage opened over it.
    pub(crate) fn run_shown(&self) -> Option<&RunId> {
        match self {
            Stage::Run(run) => Some(&run.run),
            _ => None,
        }
    }

    /// The reach of the field this stage reads, which the state follows wherever it
    /// stands: the field's own, and here for the first request, which waits on a run.
    pub(crate) fn field_reach(&self) -> Option<FieldReach> {
        match self {
            Stage::Field(field) => Some(field.reach),
            Stage::FirstRun(_) => Some(FieldReach::Here),
            _ => None,
        }
    }

    /// What the stage offers over what `followed` keeps at `now`, each with the key that
    /// takes it here: what a palette over it lists first. A stage whose prompt is the act,
    /// a decision's card, a question, a form, offers none of its own.
    pub(crate) fn offered(&self, followed: &FollowedRuns, now: DateTime<Utc>) -> Vec<Act> {
        match self {
            Stage::Field(field) => field.offered(followed, now),
            Stage::Run(run) => run.offered(followed, now),
            Stage::Catalog(catalog) => catalog.offered(),
            Stage::Evidence(evidence) => evidence.offered(),
            Stage::Doctor(doctor) => doctor.offered(),
            Stage::Upkeep(upkeep) => upkeep.offered(),
            Stage::Signpost
            | Stage::Decision(_)
            | Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Setup(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Schema(_) => Vec::new(),
        }
    }

    /// The stage as a screen of what `followed` keeps at `now`.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Screen {
        match self {
            Stage::Signpost => signpost::screen(look),
            Stage::Field(field) => field.screen(look, followed, now),
            Stage::Run(run) => run.screen(look, followed, now),
            Stage::Decision(decision) => decision.screen(look),
            Stage::FirstRun(first_run) => first_run.screen(look),
            Stage::Start(start) => start.screen(look),
            Stage::Setup(setup) => setup.screen(look),
            Stage::Catalog(catalog) => catalog.screen(look),
            Stage::Evidence(evidence) => evidence.screen(look),
            Stage::Stats(stats) => stats.screen(look, followed, now),
            Stage::Graph(graph) => graph.screen(look, followed, now),
            Stage::Doctor(doctor) => doctor.screen(look),
            Stage::Upkeep(upkeep) => upkeep.screen(look),
            Stage::Schema(schema) => schema.screen(look),
        }
    }

    /// What the stage asks for as it opens.
    pub(crate) fn opening(&self) -> Vec<Want> {
        match self {
            Stage::Signpost | Stage::Field(_) => Vec::new(),
            Stage::Run(run) => run.opening(),
            Stage::Decision(decision) => decision.opening(),
            Stage::FirstRun(first_run) => first_run.opening(),
            Stage::Setup(setup) => setup.opening(),
            Stage::Catalog(catalog) => catalog.opening(),
            Stage::Evidence(evidence) => evidence.opening(),
            Stage::Stats(stats) => stats.opening(),
            Stage::Graph(graph) => graph.opening(),
            Stage::Doctor(doctor) => doctor.opening(),
            Stage::Upkeep(upkeep) => upkeep.opening(),
            Stage::Start(_) | Stage::Schema(_) => Vec::new(),
        }
    }

    /// The ticket the performer holds what the stage's work found under, where it holds
    /// anything: what leaving the stage lets go.
    pub(crate) fn held(&self) -> Option<Ticket> {
        match self {
            Stage::Setup(setup) => setup.held(),
            Stage::Catalog(catalog) => catalog.held(),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Run(_)
            | Stage::Decision(_)
            | Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Evidence(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Doctor(_)
            | Stage::Upkeep(_)
            | Stage::Schema(_) => None,
        }
    }

    /// What the stage asks for when a person comes back to it, `followed` what the state
    /// keeps of what it follows.
    pub(crate) fn returned(&mut self, followed: &FollowedRuns) -> Vec<Want> {
        match self {
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Decision(_)
            | Stage::Start(_)
            | Stage::Setup(_)
            | Stage::Evidence(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Upkeep(_)
            | Stage::Schema(_) => Vec::new(),
            Stage::Doctor(doctor) => doctor.returned(),
            Stage::Catalog(catalog) => catalog.opening(),
            Stage::FirstRun(first_run) => first_run.returned(followed),
            Stage::Run(run) => run.returned(followed),
        }
    }

    /// What the stage the person is at asks once the state folded a piece of what it
    /// follows into `followed`: a run's stage, the page it shows read again where the run
    /// turned. Not [`Stage::heard`], news of work the stage asked for.
    pub(crate) fn followed_on(&mut self, followed: &FollowedRuns) -> Vec<Want> {
        match self {
            Stage::Run(run) => run.followed_on(followed),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Decision(_)
            | Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Setup(_)
            | Stage::Catalog(_)
            | Stage::Evidence(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Doctor(_)
            | Stage::Upkeep(_)
            | Stage::Schema(_) => Vec::new(),
        }
    }

    /// The stage, where it shows `run`, moved onto `by`, the successor a promotion handed
    /// the run's work to: a run's stage shows the successor's own page from its first
    /// line, which it reads once told what was folded ([`Stage::followed_on`]) or as the
    /// person comes back to it ([`Stage::returned`]); a run's stats and its graph show the
    /// successor's, from their first line. Not a stage opened on `by`, which stands on it
    /// from nothing.
    pub(crate) fn succeeded(&mut self, run: &RunId, by: &RunId) {
        match self {
            Stage::Run(stage) => stage.succeeded(run, by),
            Stage::Stats(stats) => stats.succeeded(run, by),
            Stage::Graph(graph) => graph.succeeded(run, by),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Decision(_)
            | Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Setup(_)
            | Stage::Catalog(_)
            | Stage::Evidence(_)
            | Stage::Doctor(_)
            | Stage::Upkeep(_)
            | Stage::Schema(_) => {}
        }
    }

    /// What `stroke` asks for here, over what `followed` keeps at `now`.
    pub(crate) fn pressed(
        &mut self,
        stroke: Stroke,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        match self {
            Stage::Signpost => signpost::pressed(stroke),
            Stage::Field(field) => field.pressed(stroke, followed, now),
            Stage::Run(run) => run.pressed(stroke, followed, now),
            Stage::Decision(decision) => decision.pressed(stroke),
            Stage::FirstRun(first_run) => first_run.pressed(stroke),
            Stage::Start(start) => start.pressed(stroke),
            Stage::Setup(setup) => setup.pressed(stroke),
            Stage::Catalog(catalog) => catalog.pressed(stroke),
            Stage::Evidence(evidence) => evidence.pressed(stroke),
            Stage::Stats(stats) => stats.pressed(stroke),
            Stage::Graph(graph) => graph.pressed(stroke),
            Stage::Doctor(doctor) => doctor.pressed(stroke),
            Stage::Upkeep(upkeep) => upkeep.pressed(stroke),
            Stage::Schema(schema) => schema.pressed(stroke),
        }
    }

    /// What an act the stage asked to be confirmed does once a person confirms it: its
    /// work, asked for by the stage, which may stand differently while it runs.
    pub(crate) fn confirmed(&mut self, operation: Operation) -> Vec<Want> {
        match self {
            Stage::Doctor(doctor) => doctor.confirmed(operation),
            Stage::Upkeep(upkeep) => upkeep.confirmed(operation),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Run(_)
            | Stage::Decision(_)
            | Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Setup(_)
            | Stage::Catalog(_)
            | Stage::Evidence(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Schema(_) => vec![Want::Work(operation)],
        }
    }

    /// Whether a field is open here.
    pub(crate) fn typing(&self) -> bool {
        match self {
            Stage::Decision(decision) => decision.typing(),
            Stage::FirstRun(first_run) => first_run.typing(),
            Stage::Start(start) => start.typing(),
            Stage::Setup(setup) => setup.typing(),
            Stage::Catalog(catalog) => catalog.typing(),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Run(_)
            | Stage::Evidence(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Doctor(_)
            | Stage::Upkeep(_)
            | Stage::Schema(_) => false,
        }
    }

    /// The page `page`, which this stage asked a surface to show, could not be shown: the
    /// evidence stage says so, the one stage that shows pages.
    pub(crate) fn unshown(&mut self, page: &std::path::Path) {
        if let Stage::Evidence(evidence) = self {
            evidence.unshown(page);
        }
    }

    /// What a surface laid out of the screen's regions, which the stage reads on the
    /// next stroke where it scrolls one: a decision's desk and a page in the main region,
    /// a run's page beside its nodes.
    pub(crate) fn laid(&mut self, laid: ScreenLaid) {
        match self {
            Stage::Decision(decision) => decision.laid(laid.main),
            Stage::Run(run) => run.laid(laid.main),
            Stage::Evidence(evidence) => evidence.laid(laid.main),
            Stage::Stats(stats) => stats.laid(laid.main),
            Stage::Graph(graph) => graph.laid(laid.main),
            Stage::Doctor(doctor) => doctor.laid(laid.main),
            Stage::Upkeep(upkeep) => upkeep.laid(laid.main),
            Stage::Schema(schema) => schema.laid(laid.main),
            Stage::Catalog(catalog) => catalog.laid(laid.main),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Setup(_) => {}
        }
    }

    /// What news of lasting work the stage asked for does here: the cases', which the
    /// catalog asks to run. Not what the state follows, which no stage asks for.
    pub(crate) fn heard(&mut self, news: News) -> Vec<Want> {
        match self {
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Run(_)
            | Stage::Decision(_)
            | Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Setup(_)
            | Stage::Evidence(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Doctor(_)
            | Stage::Upkeep(_)
            | Stage::Schema(_) => Vec::new(),
            Stage::Catalog(catalog) => catalog.heard(news),
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
            Stage::Setup(setup) => setup.answered(outcome),
            Stage::Catalog(catalog) => catalog.answered(outcome),
            Stage::Evidence(evidence) => evidence.answered(outcome),
            Stage::Stats(stats) => stats.answered(outcome),
            Stage::Graph(graph) => graph.answered(outcome),
            Stage::Doctor(doctor) => doctor.answered(outcome),
            Stage::Upkeep(upkeep) => upkeep.answered(outcome),
            Stage::Schema(_) => Vec::new(),
        }
    }

    /// What a piece of work the stage asked for remarks while it works.
    pub(crate) fn remarked(&mut self, remark: Remark) {
        match self {
            Stage::Start(start) => start.remarked(remark),
            Stage::Catalog(catalog) => catalog.remarked(remark),
            Stage::Doctor(doctor) => doctor.remarked(remark),
            Stage::Upkeep(upkeep) => upkeep.remarked(remark),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Run(_)
            | Stage::Decision(_)
            | Stage::FirstRun(_)
            | Stage::Setup(_)
            | Stage::Evidence(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Schema(_) => {}
        }
    }
}

#[cfg(test)]
mod tests;
