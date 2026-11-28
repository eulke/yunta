//! See [`super`]. What the work a stage asked for hands back to it, wherever the stage stands:
//! what lasting work found while it lasts, what it remarks while it works, and what it came
//! to; what an act no standing stage asked came to, noted where the person is; and the work
//! the stage's screen says is under way, which the state waits on while it says so.

use yunta_api::Remark;

use super::{Stage, Want};
use crate::{News, Operation, Outcome};

impl Stage {
    /// The work the stage's screen says is under way, as it was asked, in a note the screen
    /// stands on while it waits: what the state waits on while the screen says so, a reading
    /// or an act. None where the screen says nothing is, or where what it waits on is what the
    /// state follows (the runs, a run's numbers, its chronicle or its graph), which no work
    /// answers. Not the work the state waits on, which a stage under the one on screen asks too.
    pub(crate) fn shown_under_way(&self) -> Option<Operation> {
        match self {
            Stage::Doctor(doctor) => doctor.shown_under_way(),
            Stage::Upkeep(upkeep) => upkeep.shown_under_way(),
            Stage::Catalog(catalog) => catalog.shown_under_way(),
            Stage::Setup(setup) => setup.shown_under_way(),
            Stage::Evidence(evidence) => evidence.shown_under_way(),
            Stage::Stats(stats) => stats.shown_under_way(),
            Stage::Graph(graph) => graph.shown_under_way(),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Run(_)
            | Stage::Chronicle(_)
            | Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Schema(_) => None,
        }
    }

    /// What news of lasting work the stage asked for does here, wherever it stands: the
    /// cases', which the catalog asks to run. Not what the state follows, which no stage asks
    /// for.
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

    /// What an answer to the work `asked`, which the stage asked for, does here, wherever the
    /// stage stands: a run's stage tells by it an answer given on its prompt in place, and the
    /// chain it verified, from any other act it took. Not [`Stage::noted`], what an act no
    /// standing stage asked came to.
    pub(crate) fn answered(&mut self, asked: &Operation, outcome: Outcome) -> Vec<Want> {
        match self {
            Stage::Signpost => Vec::new(),
            Stage::Field(field) => field.answered(outcome),
            Stage::Run(run) => run.answered(asked, outcome),
            Stage::Chronicle(chronicle) => chronicle.answered(outcome),
            Stage::FirstRun(first_run) => first_run.answered(outcome),
            Stage::Start(start) => start.answered(asked, outcome),
            Stage::Setup(setup) => setup.answered(outcome),
            Stage::Catalog(catalog) => catalog.answered(asked, outcome),
            Stage::Evidence(evidence) => evidence.answered(outcome),
            Stage::Stats(stats) => stats.answered(outcome),
            Stage::Graph(graph) => graph.answered(outcome),
            Stage::Doctor(doctor) => doctor.answered(outcome),
            Stage::Upkeep(upkeep) => upkeep.answered(outcome),
            Stage::Schema(_) => Vec::new(),
        }
    }

    /// What an act came to that every stage that asked it left, noted here where the person
    /// is: the field and a run's screen note what an act on a run came to where it did not do
    /// what was asked, as they note their own; no other stage says what it never asked. Not
    /// [`Stage::answered`], an answer to the stage's own work.
    pub(crate) fn noted(&mut self, outcome: &Outcome) {
        match self {
            Stage::Field(field) => field.noted(outcome),
            Stage::Run(run) => run.noted(outcome),
            Stage::Signpost
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
            | Stage::Schema(_) => {}
        }
    }

    /// What a piece of work the stage asked for remarks while it works: a new run's admission
    /// says its cautions on the printed page alone.
    pub(crate) fn remarked(&mut self, remark: Remark) {
        match self {
            Stage::Catalog(catalog) => catalog.remarked(remark),
            Stage::Doctor(doctor) => doctor.remarked(remark),
            Stage::Upkeep(upkeep) => upkeep.remarked(remark),
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
            | Stage::Schema(_) => {}
        }
    }
}
