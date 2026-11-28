//! See [`super`]. What help lists over a stage: over a run, its keys and its scene's acts;
//! over the field, every key it takes; over any other stage, the keys render lists for it
//! ([`StageKeys`]): those its legend names, those it takes beyond them, and those every stage
//! answers to wherever each acts there (a new run, what needs the person, the keys that scroll
//! a page, where `q` goes down to). Each stage says here what it takes beyond its legend, so
//! help never leaves out a key the stage takes and never says one it does not.

use chrono::{DateTime, Utc};
use surco::components::Legend;
use surco::doc::Doc;
use yunta_render::screen::{GoingDown, StageKeys};

use super::Stage;
use crate::FollowedRuns;

impl Stage {
    /// The keys help lists over the stage, whose screen's legend is `legend`, of what
    /// `followed` keeps at `now`, `q` going `down` from it where it does not leave: over a
    /// run, its keys and its scene's acts in render's words; over the field, every key it
    /// takes, those its legend leaves out among them, as far and as deep as it reads; over
    /// any other stage, what [`Stage::keys`] says.
    pub(crate) fn help(
        &self,
        legend: &Legend,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
        down: GoingDown,
    ) -> Doc {
        match self {
            Stage::Run(run) => run.help(legend, followed, now),
            Stage::Field(field) => field.help(followed),
            other => yunta_render::screen::help(other.keys(legend, followed, down)),
        }
    }

    /// The keys of a stage that is neither a run nor the field, whose legend is `legend`, as
    /// help lists them: a page takes a new run by `n` where one can be asked for, goes where
    /// the attention row names by `a`, scrolls, and goes `down` by `q`; the machine opens
    /// upkeep by `u`; the workflows, the packs' listing and the pages in their place take what
    /// they say (`CatalogStage::keys`); a setup what its setting takes (`SetupStage::keys`);
    /// the signpost leaves by Escape too. A screen a line is typed on opens no help.
    fn keys<'l>(
        &self,
        legend: &'l Legend,
        followed: &FollowedRuns,
        down: GoingDown,
    ) -> StageKeys<'l> {
        let page = StageKeys {
            legend,
            beyond: Legend::new(),
            new_run: followed.can_ask_for_a_run(),
            attends: true,
            scrolls: true,
            down: Some(down),
        };
        match self {
            Stage::Signpost => StageKeys {
                beyond: yunta_render::screen::signpost_keys_beyond(),
                new_run: false,
                attends: false,
                scrolls: false,
                down: None,
                ..page
            },
            Stage::Doctor(doctor) => StageKeys {
                beyond: yunta_render::screen::machine_keys_beyond(doctor.sessions_open()),
                ..page
            },
            Stage::Catalog(catalog) => catalog.keys(page, followed.can_ask_for_a_run()),
            Stage::Setup(setup) => setup.keys(page),
            Stage::Upkeep(_)
            | Stage::Stats(_)
            | Stage::Graph(_)
            | Stage::Evidence(_)
            | Stage::Chronicle(_)
            | Stage::Schema(_) => page,
            Stage::Field(_) | Stage::Run(_) | Stage::FirstRun(_) | Stage::Start(_) => StageKeys {
                new_run: false,
                attends: false,
                scrolls: false,
                down: None,
                ..page
            },
        }
    }
}
