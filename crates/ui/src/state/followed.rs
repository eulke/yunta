//! See [`super`]. What the state follows, a function of where the person is; what a
//! piece of it folded asks: a promotion's successor in its run's place under every stage
//! while the run leads its screen, the run kept with its successor beside it while a
//! person holds it, the person called for each run it turned to need them, where the
//! application first stands in a repository set up, once what it keeps of the field is
//! first read, and what the stage the person is at asks of it; when what it keeps stands
//! caught up with the log; and the scene the run's screen the person is at stands as, of
//! the run it keeps, and where its pane stands.

use std::collections::BTreeMap;

use surco::doc::Depth;
use yunta_api::{FieldReach, FollowNews, Followed, Repository};
use yunta_core::{RunId, Seq};

use super::calling::Moved;
use super::State;
use crate::stage::{FieldStage, FirstRunStage};
use crate::{FollowedRuns, ItsRun, Request, Scene, Scroll, Stage};

impl State {
    /// How deep the field the person is at lists its runs: at a glance, or with the closed
    /// too; none where they are at another stage, a field under it among them. A field that
    /// holds no closed run draws alike at either depth, and `d` and the runs that close later
    /// still tell the two apart. Not its reach, which [`State::followed`] names.
    pub fn field_depth(&self) -> Option<Depth> {
        self.stage().and_then(Stage::field_depth)
    }

    /// What the application follows: the field at the field's stage's reach wherever the
    /// field or the first request stands, and here before any stage stands in a repository
    /// set up; the run of the nearest run's stage, under every stage over it; nothing else.
    pub fn followed(&self) -> Followed {
        let stages = || self.stage().into_iter().chain(self.stages_under().rev());
        let set_up = self.stage.is_none()
            && matches!(
                self.followed_runs.repository,
                Some(Repository::SetUp { .. })
            );
        let field = stages().find_map(Stage::field_reach);
        Followed {
            field: field.or(set_up.then_some(FieldReach::Here)),
            run: stages().find_map(Stage::run_shown).cloned(),
        }
    }

    /// `news` of what the state follows folded into what it keeps, and what that asks:
    /// every stage that shows a run moved onto the successor a promotion handed its work
    /// to, where the news hands it and the run leads its screen, and the successor followed;
    /// where the application first stands, while it stands nowhere; what the stage the
    /// person is at asks of what was folded; then the person called once for each run the
    /// news turned to need them.
    pub(super) fn folded(&mut self, news: FollowNews) -> Vec<Request> {
        let moved = Moved::by(&news);
        let mut asked = Vec::new();
        if let Some((run, by)) = self.followed_runs.took(news) {
            asked.extend(self.succeeded(&run, &by));
        }
        let calls = self.calling.folded(moved, &self.followed_runs, self.now);
        asked.extend(self.stands_on_first_news());
        let by = self.on_screen();
        let wants = match self.stage.as_mut() {
            Some(standing) => standing.stage.followed_on(&self.followed_runs),
            None => Vec::new(),
        };
        asked.extend(self.wanted(by, wants));
        asked.extend(calls);
        asked
    }

    /// Every stage that shows `run`, the one the person is at and each under it, moved
    /// onto `by`, the successor a promotion handed the run's work to, where the run leads
    /// its screen: the successor is what is followed from then on, and it takes the place
    /// of the run in view at once, so the stage the person is at reads it as it is told
    /// what was folded, and one under as the person comes back to it. Where a person holds
    /// the run's screen, every stage stays on the run, which keeps its successor beside it
    /// until the person gives the lead back. What following the successor asks.
    fn succeeded(&mut self, run: &RunId, by: &RunId) -> Option<Request> {
        let held = (self.stage().into_iter().chain(self.stages_under())).any(|stage| match stage {
            Stage::Run(shown) => stage.run_shown() == Some(run) && !shown.leads(),
            _ => false,
        });
        if held {
            return None;
        }
        for standing in self.stage.iter_mut().chain(self.under.iter_mut()) {
            standing.stage.succeeded(run, by);
        }
        self.followed_runs.asked(self.followed())
    }

    /// Where the application first stands in a repository set up, once the field's first
    /// news is folded: on the field where it holds a run, read or not, on the first request
    /// where it holds none. Nothing before that news, nor once it stands anywhere.
    fn stands_on_first_news(&mut self) -> Vec<Request> {
        let unplaced = self.stage.is_none()
            && matches!(
                self.followed_runs.repository,
                Some(Repository::SetUp { .. })
            );
        let Some(field) = self.followed_runs.field.as_ref().filter(|_| unplaced) else {
            return Vec::new();
        };
        let stage = match field.holds_a_run() {
            true => Stage::Field(FieldStage::reaching(field.reach)),
            false => Stage::FirstRun(FirstRunStage::new()),
        };
        self.stands_on(stage)
    }

    /// Whether what it follows is read at `heads`, each run's last seq in the log: the field
    /// and the run read whole, every run kept at its head, what the run's nodes handed over
    /// kept, what a run parked on a person with a menu or a round waits for read, and a run
    /// closed promoted handed on to its successor or none come. What a test asks before it
    /// gives a stroke.
    pub fn caught_up(&self, heads: &BTreeMap<RunId, Seq>) -> bool {
        let followed = self.followed();
        self.followed_runs.caught_up(&followed, heads)
    }

    /// What the state keeps of what it follows.
    pub fn followed_runs(&self) -> &FollowedRuns {
        &self.followed_runs
    }

    /// The scene the screen of the run the person is at stands as, at the instant the
    /// state stands at: none where they are at no run's screen, or before its run is read.
    pub fn scene(&self) -> Option<Scene<'_>> {
        match self.stage()? {
            Stage::Run(run) => run.scene(&self.followed_runs, self.now),
            _ => None,
        }
    }

    /// Where the pane of the run's screen the person is at stands, at the instant the state
    /// stands at: where a person scrolled what it shows to, the feed it ends with or its own,
    /// and whether the last surface left it anything to scroll, which no row of the screen
    /// says; or, at a page that scrolls, where that page stands, which no row of its screen
    /// says either, its legend the same whether it scrolls or not: a run's numbers, a
    /// workflow's past runs, a graph, a run's or a workflow's, what a run leaves, the machine,
    /// upkeep, a page in the workflows' place, a document's shape, the setup's audit. None
    /// anywhere else.
    pub fn pane(&self) -> Option<Scroll> {
        match self.stage()? {
            Stage::Run(run) => Some(run.pane(&self.followed_runs, self.now)),
            Stage::Stats(stats) => Some(stats.page()),
            Stage::Graph(graph) => Some(graph.page()),
            Stage::Evidence(evidence) => Some(evidence.page()),
            Stage::Doctor(doctor) => Some(doctor.page()),
            Stage::Upkeep(upkeep) => Some(upkeep.page()),
            Stage::Catalog(catalog) => catalog.page(),
            Stage::Schema(schema) => schema.page(),
            Stage::Setup(setup) => setup.page(),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Chronicle(_)
            | Stage::FirstRun(_)
            | Stage::Start(_) => None,
        }
    }

    /// What the stage the person is at is to its run, under any palette or help open over
    /// it: a view of the run as it moves, or a record of what was written for it; none at a
    /// stage over no one run.
    pub fn its_run(&self) -> Option<ItsRun> {
        self.stage().and_then(Stage::its_run)
    }
}
