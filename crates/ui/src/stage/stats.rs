//! See [`super`]. What a run cost, opened from the run with `s`: its facts, where its time
//! went and who spent it, as a run's screen tells them, what only the printed page has room
//! for left to `yunta stats`; in the run's frame (`over_the_run`), where `a` goes where its
//! attention row names and `n` opens a new run over it. Or what a
//! workflow's past runs cost and what their checks never caught, as `yunta stats
//! --workflow` shows it, told as a run's numbers are, opened from the catalog with `h`, in
//! the frame of the field it was opened over (`over_the_field`), where `a` and `n` do the
//! same.
//! Either scrolls where it is taller than what shows of it. Escape goes back to where it was
//! opened, and `q` down to the field.
//!
//! A run's stats are made at each draw from the run in view, which the state keeps of
//! what it follows, framed at the instant of the draw and costed at the prices the
//! repository's reading said: they move with the run as its news come, and nothing is
//! read for them. Until the run in view is the stage's run, they are being read. A
//! promotion moves the stage onto the successor (`succeeded`), whose stats start at
//! their first line. A workflow's past runs are read once, as the stage opens.

use chrono::{DateTime, Utc};
use surco::components::{Laid, Legend, Note, Region, Screen};
use surco::doc::Doc;
use surco::look::Look;
use yunta_api::HistoryReading;
use yunta_core::step::{Arg, NextStep};
use yunta_core::{RunId, WorkflowName};
use yunta_render::page::{History, Page, Stats, StatsTold};
use yunta_render::screen::{self, PageRead};
use yunta_replay::stats::RunStats;

use super::over_the_field::{attending_over_the_field, in_the_fields_frame, KeyA};
use super::over_the_run::{attending, in_the_runs_frame};
use super::{ItsRun, Want};
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{FollowedRuns, Operation, Outcome};

/// What a run cost, or what a workflow's past runs did.
#[derive(Debug, Clone)]
pub struct StatsStage {
    spending: Spending,
    /// Where the page is scrolled to.
    scroll: Scroll,
    /// What refused or failed, where something did.
    said: Option<String>,
}

/// Whose spending stands: one run's, made from the run in view at each draw, or every
/// past run of a workflow's, with what was read of them once it is.
#[derive(Debug, Clone)]
enum Spending {
    Run(RunId),
    Workflow {
        workflow: WorkflowName,
        /// The name the catalog offers the workflow by, which the trail names it by, where
        /// the stage was opened from the catalog; its own name otherwise.
        offered_as: Option<String>,
        read: Option<Box<HistoryReading>>,
    },
}

impl StatsStage {
    /// What `run` cost, made from the run in view once it is `run`.
    pub(crate) fn of_run(run: RunId) -> Self {
        StatsStage::of(Spending::Run(run))
    }

    /// The past runs of the workflow called `workflow`, yet to be read.
    pub(crate) fn of_workflow(workflow: WorkflowName) -> Self {
        StatsStage::of(Spending::Workflow {
            workflow,
            offered_as: None,
            read: None,
        })
    }

    /// This stage, where it shows a workflow's past runs, opened from the catalog, which
    /// offers the workflow as `name`: its trail names it so.
    pub(crate) fn offered_as(mut self, name: String) -> Self {
        if let Spending::Workflow { offered_as, .. } = &mut self.spending {
            *offered_as = Some(name);
        }
        self
    }

    fn of(spending: Spending) -> Self {
        StatsStage {
            spending,
            scroll: Scroll::default(),
            said: None,
        }
    }

    /// What it is to its run, where it tells a run's numbers: a view of it as it moves.
    pub(super) fn its_run(&self) -> Option<ItsRun> {
        match &self.spending {
            Spending::Run(run) => Some(ItsRun::Shows(run.clone())),
            Spending::Workflow { .. } => None,
        }
    }

    /// Where the page stands, a run's numbers or a workflow's past runs, which no row of its
    /// screen says, its legend the same whether it scrolls or not.
    pub(crate) fn page(&self) -> Scroll {
        self.scroll
    }

    /// What the stage asks for as it opens: a workflow's past runs, read; nothing for a
    /// run's, which the run in view makes.
    pub(crate) fn opening(&self) -> Vec<Want> {
        match &self.spending {
            Spending::Run(_) => Vec::new(),
            Spending::Workflow { workflow, .. } => vec![Want::Work(past_runs_read(workflow))],
        }
    }

    /// The work its screen says is under way: a workflow's past runs read, until they are.
    pub(crate) fn shown_under_way(&self) -> Option<Operation> {
        match &self.spending {
            Spending::Workflow {
                workflow,
                read: None,
                ..
            } => Some(past_runs_read(workflow)),
            Spending::Workflow { read: Some(_), .. } | Spending::Run(_) => None,
        }
    }

    /// A run's stats as a run's screen tells them of the run in view `followed` keeps at
    /// `now`, at the prices it keeps, in the run's frame, a line typed in what stands over it
    /// where `typed`; or what was read of a workflow's past runs; or that either is being
    /// read; what refused or failed; with the keys.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
        typed: bool,
    ) -> Screen {
        let (mut doc, trail) = match &self.spending {
            Spending::Run(run) => {
                let doc = self.run_spent(run, look, followed, now);
                let trail = vec![screen::place_of(run.handle(), screen::STATS)];
                (doc, trail)
            }
            Spending::Workflow {
                workflow,
                offered_as,
                read,
            } => {
                let named = offered_as.as_deref().unwrap_or(workflow.as_str());
                let trail = vec![
                    screen::WORKFLOWS.to_string(),
                    screen::place_of(named, screen::HISTORY),
                ];
                let doc = past_runs(workflow, read.as_deref(), look);
                (doc, trail)
            }
        };
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let region = match &self.spending {
            Spending::Run(_) => Region::new(doc),
            Spending::Workflow { .. } => Region::of_a_page(doc),
        };
        let mut shown = Screen::new(region.in_sight(self.scroll.sight()));
        shown.trail = std::iter::once(screen::YUNTA.to_string())
            .chain(trail)
            .collect();
        let new_run = followed.can_ask_for_a_run();
        shown.legend = screen::page_legend(PageRead::Whole, Legend::new(), new_run);
        match &self.spending {
            Spending::Run(run) => in_the_runs_frame(&mut shown, run, followed, now, typed),
            Spending::Workflow { .. } => {
                in_the_fields_frame(&mut shown, followed, now, typed, KeyA::Attends);
            }
        }
        shown
    }

    /// What `run` cost as a run's screen tells it, made from the run in view `followed`
    /// keeps, at `now` and at the prices it keeps; that it is being read until the run in
    /// view is `run`.
    fn run_spent(
        &self,
        run: &RunId,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Doc {
        let in_view = followed.in_view.as_deref();
        let Some(in_view) = in_view.filter(|in_view| in_view.run.run() == run) else {
            return Doc::new().with(Note(screen::READING_THE_STATS.to_string()));
        };
        let kept = &in_view.run;
        Stats {
            frame: kept.frame(),
            stats: &RunStats::at(kept.state(), &kept.manifest().workflow, now),
            pricing: followed.prices.as_ref(),
            told: StatsTold::OnScreen,
        }
        .document(look)
    }

    /// The stage, where it shows `run`'s stats, moved onto `by`, the successor a promotion
    /// handed the work to: its stats, from their first line, once the run in view is `by`.
    pub(crate) fn succeeded(&mut self, run: &RunId, by: &RunId) {
        if let Spending::Run(shown) = &mut self.spending {
            if shown == run {
                *shown = by.clone();
                self.scroll.rewound();
            }
        }
    }

    /// What `stroke` asks for here, over what `followed` keeps at `now`: `a` goes where the
    /// attention row names and `n` opens a new run over the page; Escape goes back, `q` goes
    /// down to the field, Ctrl-C leaves, and the keys that scroll scroll the page.
    pub(crate) fn pressed(
        &mut self,
        stroke: Stroke,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        match (stroke, &self.spending) {
            (Stroke::Insert('a'), Spending::Run(run)) => attending(run, followed, now),
            (Stroke::Insert('a'), Spending::Workflow { .. }) => {
                attending_over_the_field(followed, now)
            }
            (Stroke::Insert('n'), _) if followed.can_ask_for_a_run() => {
                Want::taking(&NextStep::Run {
                    workflow: Arg::Blank,
                })
            }
            (Stroke::Decline, _) => vec![Want::Back],
            (Stroke::Insert('q'), _) => vec![Want::ToTheField(None)],
            (Stroke::Interrupt, _) => vec![Want::Leave],
            (other, _) => {
                self.scroll.pressed(other);
                Vec::new()
            }
        }
    }

    /// What a surface laid out of the page, which the keys that scroll move from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        self.scroll.laid(laid);
    }

    /// What an answer to the stage's work does: a workflow's past runs read stand; what
    /// refused or failed is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match (outcome, &mut self.spending) {
            (Outcome::History(past), Spending::Workflow { read, .. }) => *read = Some(past),
            (Outcome::Refused(refused), _) => self.said = Some(refused.to_string()),
            (Outcome::Failed(said), _) => self.said = Some(said),
            (
                Outcome::History(_)
                | Outcome::Graph(_)
                | Outcome::Doctor(_)
                | Outcome::Collected(_)
                | Outcome::Repository(_)
                | Outcome::Catalog(_)
                | Outcome::Workflow(_)
                | Outcome::Pack(_)
                | Outcome::Evidence(_)
                | Outcome::PullRequest(_)
                | Outcome::Decided { .. }
                | Outcome::Answered { .. }
                | Outcome::Started { .. }
                | Outcome::Resumed { .. }
                | Outcome::Cancelled { .. }
                | Outcome::Closed { .. }
                | Outcome::Detected { .. }
                | Outcome::SetupPack { .. }
                | Outcome::Initialized { .. }
                | Outcome::Stopped,
                _,
            ) => {}
        }
        Vec::new()
    }
}

/// What the past runs of the workflow called `workflow` did, as `yunta stats --workflow`
/// shows it, once `read`; that they are being read until then.
fn past_runs(workflow: &WorkflowName, read: Option<&HistoryReading>, look: &Look) -> Doc {
    match read {
        Some(past) => History {
            name: workflow,
            history: &past.summaries,
            findings: past.findings.as_ref(),
        }
        .document(look),
        None => Doc::new().with(Note(screen::READING_THE_HISTORY.to_string())),
    }
}

/// The work that reads the past runs of the workflow called `workflow`.
fn past_runs_read(workflow: &WorkflowName) -> Operation {
    Operation::ReadHistory {
        workflow: workflow.clone(),
    }
}
