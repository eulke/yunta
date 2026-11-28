//! See [`super`]. What a stage offers, as the palette lists it over the stage before what
//! every stage offers where the application stands: its own acts, each with the key that
//! takes it here, then, on a page, what it offers by the keys every page answers to.

use chrono::{DateTime, Utc};
use yunta_core::step::{Arg, NextStep};

use super::over_the_field;
use super::over_the_run;
use super::{signpost, Act, ItsRun, Stage};
use crate::prompt::Stroke;
use crate::FollowedRuns;

impl Stage {
    /// What the stage offers over what `followed` keeps at `now`, each with the key that
    /// takes it here: what a palette over it lists first, a page's own acts then what it
    /// offers by the keys every page answers to ([`Stage::offered_by_every_page`]). A stage
    /// whose prompt is the act, a form or a question, offers none of its own.
    pub(crate) fn offered(&self, followed: &FollowedRuns, now: DateTime<Utc>) -> Vec<Act> {
        let own = self.offered_by_itself(followed, now);
        let every_page = self.offered_by_every_page(followed, now);
        own.into_iter().chain(every_page).collect()
    }

    /// The acts a page offers by the keys every page answers to, each where its key takes it
    /// there: going where the attention row names, by `a`, where it names a run; a new run,
    /// by `n`, where one can be asked for; neither where the page takes the key for an act of
    /// its own, as the packs' listing and the workflows do. None on the field, a run, a setup,
    /// the signpost or a screen a line is typed on, which offer their own or none.
    fn offered_by_every_page(&self, followed: &FollowedRuns, now: DateTime<Utc>) -> Vec<Act> {
        let a_page = match self {
            Stage::Doctor(_)
            | Stage::Upkeep(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Evidence(_)
            | Stage::Chronicle(_) => true,
            Stage::Schema(schema) => !schema.typing(),
            Stage::Catalog(catalog) => !catalog.typing(),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Run(_)
            | Stage::Setup(_)
            | Stage::FirstRun(_)
            | Stage::Start(_) => false,
        };
        if !a_page {
            return Vec::new();
        }
        let takes_itself =
            |key| matches!(self, Stage::Catalog(catalog) if catalog.takes_itself(key));
        let attended = match self.its_run() {
            Some(ItsRun::Shows(run) | ItsRun::WroteFor(run)) => {
                over_the_run::attends(&run, followed, now)
            }
            None => over_the_field::attends(followed, now),
        };
        let attending =
            (attended && !takes_itself('a')).then(|| Act::by(Stroke::Insert('a'), NextStep::Needs));
        let new_run = (followed.can_ask_for_a_run() && !takes_itself('n')).then(|| {
            let blank = NextStep::Run {
                workflow: Arg::Blank,
            };
            Act::by(Stroke::Insert('n'), blank)
        });
        attending.into_iter().chain(new_run).collect()
    }

    /// What the stage offers of its own, each with the key that takes it here.
    fn offered_by_itself(&self, followed: &FollowedRuns, now: DateTime<Utc>) -> Vec<Act> {
        match self {
            Stage::Field(field) => field.offered(followed, now),
            Stage::Run(run) => run.offered(followed, now),
            Stage::Catalog(catalog) => catalog.offered(followed),
            Stage::Evidence(evidence) => evidence.offered(),
            Stage::Doctor(doctor) => doctor.offered(),
            Stage::Upkeep(upkeep) => upkeep.offered(),
            Stage::Signpost => signpost::offered(),
            Stage::Setup(setup) => setup.offered(),
            Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Chronicle(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Schema(_) => Vec::new(),
        }
    }
}
