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
//! offers where the application stands ([`Stage::always`]), each of those taken as its step
//! is (`want`); a page offers a new run and what needs the person by their keys beside its
//! own. Help lists the keys it takes, those its legend leaves out among them (`help`). A
//! stage over a run says what it is to it
//! (`its_run`): a view of the run as it moves, or a record of what was written for it. A stage
//! whose work found something the performer holds for later work says the ticket it is held
//! under ([`Stage::held`]), which the state lets go when the stage is left. What a stage wants
//! of the state is `want`'s; what its work hands back to it, wherever it stands, `handed`'s. Each stage that joins adds its variant here and its module beside
//! this one.

mod act;
mod catalog;
mod chronicle;
mod doctor;
mod evidence;
mod field;
mod first_run;
mod graph;
mod handed;
mod help;
mod its_run;
mod offers;
mod over_the_field;
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
use surco::components::{Screen, ScreenLaid};
use surco::doc::Depth;
use surco::look::Look;

use yunta_api::FieldReach;
use yunta_core::RunId;

use crate::prompt::Stroke;
use crate::{Address, FollowedRuns, Operation, Ticket};

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
    /// A new run, its rows being set, then started.
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
    /// stands: the field's own, and here for the first request, which waits on a run, and for
    /// a setup, whose sentence opens on what the field holds and whose tally and attention row
    /// count this repository's runs.
    pub(crate) fn field_reach(&self) -> Option<FieldReach> {
        match self {
            Stage::Field(field) => Some(field.reach),
            Stage::FirstRun(_) | Stage::Setup(_) => Some(FieldReach::Here),
            _ => None,
        }
    }

    /// How deep the field this stage is lists its runs: at a glance, or with the closed too.
    /// None for any other stage. Not its reach, which what the state follows names.
    pub(crate) fn field_depth(&self) -> Option<Depth> {
        match self {
            Stage::Field(field) => Some(field.depth),
            _ => None,
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
            Stage::Field(field) => field.screen(look, followed, now, typed),
            Stage::Run(run) => run.screen(look, followed, now, typed),
            Stage::Chronicle(chronicle) => chronicle.screen(look, followed, now, typed),
            Stage::FirstRun(first_run) => first_run.screen(look, followed, now),
            Stage::Start(start) => start.screen(look, followed, now),
            Stage::Setup(setup) => setup.screen(look, followed, now, typed),
            Stage::Catalog(catalog) => catalog.screen(look, followed, now, typed),
            Stage::Evidence(evidence) => evidence.screen(look, followed, now, typed),
            Stage::Stats(stats) => stats.screen(look, followed, now, typed),
            Stage::Graph(graph) => graph.screen(look, followed, now, typed),
            Stage::Doctor(doctor) => doctor.screen(look, followed, now, typed),
            Stage::Upkeep(upkeep) => upkeep.screen(look, followed, now, typed),
            Stage::Schema(schema) => schema.screen(look, followed, now, typed),
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
            Stage::Start(start) => start.opening(),
            Stage::Chronicle(_) | Stage::Schema(_) => Vec::new(),
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
    /// keeps of what it follows: what it reads anew each time, the workflows and doctor's
    /// checks; the work it waited on while another stood over it it took the answers of under
    /// it, and nothing asks for that again.
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
            Stage::Catalog(catalog) => catalog.returned(),
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
            Stage::Setup(setup) => setup.pressed(stroke, followed, now),
            Stage::Catalog(catalog) => catalog.pressed(stroke, followed, now),
            Stage::Evidence(evidence) => evidence.pressed(stroke, followed, now),
            Stage::Stats(stats) => stats.pressed(stroke, followed, now),
            Stage::Graph(graph) => graph.pressed(stroke, followed, now),
            Stage::Doctor(doctor) => doctor.pressed(stroke, followed, now),
            Stage::Upkeep(upkeep) => upkeep.pressed(stroke, followed, now),
            Stage::Schema(schema) => schema.pressed(stroke, followed, now),
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

    /// What an act the stage asked to be confirmed does once a person confirms it, or goes
    /// back from it where that is an act of its own: its work, asked for by the stage, which
    /// may stand differently while it runs.
    pub(crate) fn confirmed(&mut self, operation: Operation) -> Vec<Want> {
        match self {
            Stage::Doctor(doctor) => doctor.confirmed(operation),
            Stage::Upkeep(upkeep) => upkeep.confirmed(operation),
            Stage::Setup(setup) => setup.confirmed(operation),
            Stage::Catalog(catalog) => catalog.confirmed(operation),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Run(_)
            | Stage::Chronicle(_)
            | Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Evidence(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Schema(_) => vec![Want::Work(operation)],
        }
    }

    /// Whether the stage's screen is one a line is typed on, over what `followed` keeps at
    /// `now`: every printable key types there, so the palette and help open nowhere on it and
    /// its keys stand alone, whichever of its rows the keys act on. A new run's always is; a
    /// setup's while a row is changed in place; any other stage's is where a line is open on
    /// it. Not [`Stage::typing`], whether a line is open where the keys act.
    pub(crate) fn typed(&self, followed: &FollowedRuns, now: DateTime<Utc>) -> bool {
        match self {
            Stage::Start(_) => true,
            Stage::Setup(setup) => setup.typed(),
            other => other.typing(followed, now),
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
            Stage::Schema(schema) => schema.typing(),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Chronicle(_)
            | Stage::Evidence(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Doctor(_)
            | Stage::Upkeep(_) => false,
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
    /// next stroke where it scrolls one: a page in the main region, a setup's pack audited
    /// among them, a run's pane beside its nodes, its feed or what is read in its prompt in
    /// place as the run `followed` keeps stands at `now`, and a run's whole chronicle.
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
            Stage::Setup(setup) => setup.laid(laid.main),
            Stage::Signpost | Stage::Field(_) | Stage::FirstRun(_) | Stage::Start(_) => {}
        }
    }
}

#[cfg(test)]
mod tests;
