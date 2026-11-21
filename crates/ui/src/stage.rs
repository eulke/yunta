//! See [`crate`]. Where a person is: one variant per stage, each holding what its
//! stage reads and where the person stands in it, in `stage::<name>`.
//!
//! A stage says what its screen shows, what it asks for as it opens and as a person
//! comes back to it, what a stroke does there and what news and answers of its work
//! do, each as what it wants of the state ([`Want`]): work, work let go, a
//! confirmation, another stage opened over it, going back, going down to the field, or
//! leaving. The state
//! carries those out; an act a confirmation stood before is handed back to the stage
//! that asked for it once confirmed ([`Stage::confirmed`]), which asks for its work.
//!
//! A stage says what it offers as [`Act`]s, each a step as a command line names it and
//! the key that takes it there, which the palette lists over it before what every stage
//! offers ([`Stage::always`]), each of those taken as its step is (`want`), and the keys
//! help lists over it ([`Stage::help`]). A stage over a run says what it is to it
//! (`its_run`): a view of the run as it moves, or a record of what was written for it. A stage
//! whose work found something the performer holds for later work says the ticket it is held
//! under ([`Stage::held`]), which the state lets go when the stage is left. What a stage wants
//! of the state is `want`'s. Each stage that joins adds its variant here and its module beside
//! this one.

mod act;
mod catalog;
mod chronicle;
mod doctor;
mod evidence;
mod field;
mod first_run;
mod graph;
mod its_run;
mod over_the_run;
mod run;
mod schema;
mod setup;
mod signpost;
mod start;
mod stats;
mod steps;
mod upkeep;
mod want;

pub(crate) use act::Act;
pub use catalog::CatalogStage;
pub use chronicle::ChronicleStage;
pub use doctor::DoctorStage;
pub use evidence::{Evidence, EvidenceStage};
pub use field::FieldStage;
pub use first_run::FirstRunStage;
pub use graph::GraphStage;
pub use its_run::ItsRun;
pub use run::{Grounds, RunStage, Scene, Standstill};
pub use schema::SchemaStage;
pub use setup::SetupStage;
pub use start::StartStage;
pub use stats::StatsStage;
pub use upkeep::UpkeepStage;
pub(crate) use want::Want;

use chrono::{DateTime, Utc};
use surco::components::{Legend, Screen, ScreenLaid};
use surco::doc::Doc;
use surco::look::Look;

use yunta_api::{FieldReach, Remark};
use yunta_core::RunId;

use crate::prompt::Stroke;
use crate::{Address, FollowedRuns, News, Operation, Outcome, Ticket};

/// Where a person is.
#[derive(Debug, Clone)]
pub enum Stage {
    /// Outside a repository: that yunta works in one, and the ways on from here.
    Signpost,
    /// The runs, and the acts on them.
    Field(FieldStage),
    /// A run, followed.
    Run(RunStage),
    /// A run's whole chronicle, a feed of its own over the run.
    Chronicle(ChronicleStage),
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
    /// a form or a question, offers none of its own.
    pub(crate) fn offered(&self, followed: &FollowedRuns, now: DateTime<Utc>) -> Vec<Act> {
        match self {
            Stage::Field(field) => field.offered(followed, now),
            Stage::Run(run) => run.offered(followed, now),
            Stage::Catalog(catalog) => catalog.offered(),
            Stage::Evidence(evidence) => evidence.offered(),
            Stage::Doctor(doctor) => doctor.offered(),
            Stage::Upkeep(upkeep) => upkeep.offered(),
            Stage::Signpost
            | Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Setup(_)
            | Stage::Chronicle(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Schema(_) => Vec::new(),
        }
    }

    /// The stage as a screen of what `followed` keeps at `now`, a line typed in what stands
    /// over it where `typed`: the palette's, where every key types.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
        typed: bool,
    ) -> Screen {
        match self {
            Stage::Signpost => signpost::screen(look),
            Stage::Field(field) => field.screen(look, followed, now),
            Stage::Run(run) => run.screen(look, followed, now, typed),
            Stage::Chronicle(chronicle) => chronicle.screen(look, followed, now, typed),
            Stage::FirstRun(first_run) => first_run.screen(look),
            Stage::Start(start) => start.screen(look),
            Stage::Setup(setup) => setup.screen(look),
            Stage::Catalog(catalog) => catalog.screen(look),
            Stage::Evidence(evidence) => evidence.screen(look),
            Stage::Stats(stats) => stats.screen(look, followed, now, typed),
            Stage::Graph(graph) => graph.screen(look, followed, now, typed),
            Stage::Doctor(doctor) => doctor.screen(look),
            Stage::Upkeep(upkeep) => upkeep.screen(look),
            Stage::Schema(schema) => schema.screen(look),
        }
    }

    /// The keys help lists over the stage, whose screen's legend is `legend`, of what
    /// `followed` keeps at `now`: over a run, its keys and its scene's acts in render's
    /// words; over any other stage, each key its legend names, then those every stage
    /// answers to.
    pub(crate) fn help(&self, legend: &Legend, followed: &FollowedRuns, now: DateTime<Utc>) -> Doc {
        match self {
            Stage::Run(run) => run.help(legend, followed, now),
            _ => yunta_render::screen::help(legend),
        }
    }

    /// What the stage asks for as it opens.
    pub(crate) fn opening(&self) -> Vec<Want> {
        match self {
            Stage::Signpost | Stage::Field(_) | Stage::Run(_) => Vec::new(),
            Stage::FirstRun(first_run) => first_run.opening(),
            Stage::Setup(setup) => setup.opening(),
            Stage::Catalog(catalog) => catalog.opening(),
            Stage::Evidence(evidence) => evidence.opening(),
            Stage::Stats(stats) => stats.opening(),
            Stage::Graph(graph) => graph.opening(),
            Stage::Doctor(doctor) => doctor.opening(),
            Stage::Upkeep(upkeep) => upkeep.opening(),
            Stage::Chronicle(_) | Stage::Start(_) | Stage::Schema(_) => Vec::new(),
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
            | Stage::Chronicle(_)
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
            | Stage::Chronicle(_)
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
    /// follows into `followed`: a run's stage, what its scene reads beyond the log where the
    /// run came to stand for it. Not [`Stage::heard`], news of work the stage asked for.
    pub(crate) fn followed_on(&mut self, followed: &FollowedRuns) -> Vec<Want> {
        match self {
            Stage::Run(run) => run.followed_on(followed),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Chronicle(_)
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
    /// the run's work to: a run's stage shows the successor's own screen, its pane from its
    /// first line, as the state follows the successor in the run's place; a run's stats and
    /// its graph show the successor's, from their first line, and its whole chronicle the
    /// successor's, from its end. Not a stage opened on `by`, which stands on it from nothing.
    pub(crate) fn succeeded(&mut self, run: &RunId, by: &RunId) {
        match self {
            Stage::Run(stage) => stage.succeeded(run, by),
            Stage::Chronicle(chronicle) => chronicle.succeeded(run, by),
            Stage::Stats(stats) => stats.succeeded(run, by),
            Stage::Graph(graph) => graph.succeeded(run, by),
            Stage::Signpost
            | Stage::Field(_)
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
            Stage::Chronicle(chronicle) => chronicle.pressed(stroke, followed, now),
            Stage::FirstRun(first_run) => first_run.pressed(stroke),
            Stage::Start(start) => start.pressed(stroke),
            Stage::Setup(setup) => setup.pressed(stroke),
            Stage::Catalog(catalog) => catalog.pressed(stroke),
            Stage::Evidence(evidence) => evidence.pressed(stroke),
            Stage::Stats(stats) => stats.pressed(stroke, followed, now),
            Stage::Graph(graph) => graph.pressed(stroke, followed, now),
            Stage::Doctor(doctor) => doctor.pressed(stroke),
            Stage::Upkeep(upkeep) => upkeep.pressed(stroke),
            Stage::Schema(schema) => schema.pressed(stroke),
        }
    }

    /// A key that opens a dialog over the stage, taken there: on a run's, the note it stood
    /// on goes, as it goes with any key, and the next remark not yet said of what `followed`
    /// keeps takes its place.
    pub(crate) fn opened_over(&mut self, followed: &FollowedRuns) {
        if let Stage::Run(run) = self {
            run.noted_away(followed);
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
            | Stage::Chronicle(_)
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

    /// Whether a field is open here, over what `followed` keeps at `now`: a run's, where
    /// words are typed on the prompt in place it holds in sight.
    pub(crate) fn typing(&self, followed: &FollowedRuns, now: DateTime<Utc>) -> bool {
        match self {
            Stage::Run(run) => run.typing(followed, now),
            Stage::FirstRun(first_run) => first_run.typing(),
            Stage::Start(start) => start.typing(),
            Stage::Setup(setup) => setup.typing(),
            Stage::Catalog(catalog) => catalog.typing(),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Chronicle(_)
            | Stage::Evidence(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Doctor(_)
            | Stage::Upkeep(_)
            | Stage::Schema(_) => false,
        }
    }

    /// What this stage asked a surface to show, `address`, could not be shown: the
    /// evidence stage says so of the page it wrote, a run's stage of the pull request's
    /// link, each with where to find it.
    pub(crate) fn unshown(&mut self, address: &Address) {
        match self {
            Stage::Evidence(evidence) => evidence.unshown(address),
            Stage::Run(run) => run.unshown(address),
            _ => {}
        }
    }

    /// What a surface laid out of the screen's regions, which the stage reads on the
    /// next stroke where it scrolls one: a page in the main region, a run's pane beside its
    /// nodes, its feed or what is read in its prompt in place as the run `followed` keeps
    /// stands at `now`, and a run's whole chronicle.
    pub(crate) fn laid(&mut self, laid: ScreenLaid, followed: &FollowedRuns, now: DateTime<Utc>) {
        match self {
            Stage::Run(run) => run.laid(laid.main, followed, now),
            Stage::Chronicle(chronicle) => chronicle.laid(laid.main),
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
            | Stage::Chronicle(_)
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

    /// What an answer to the work `asked`, which the stage asked for, does here: a run's
    /// stage tells by it an answer given on its prompt in place, and the chain it verified,
    /// from any other act it took.
    pub(crate) fn answered(&mut self, asked: &Operation, outcome: Outcome) -> Vec<Want> {
        match self {
            Stage::Signpost => Vec::new(),
            Stage::Field(field) => field.answered(outcome),
            Stage::Run(run) => run.answered(asked, outcome),
            Stage::Chronicle(chronicle) => chronicle.answered(outcome),
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
            | Stage::Chronicle(_)
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
