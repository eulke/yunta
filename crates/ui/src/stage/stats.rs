//! See [`super`]. What a run cost, as `yunta stats` shows it, opened from the run
//! with `s`; or what a workflow's past runs cost and what their checks never
//! caught, as `yunta stats --workflow` shows it, opened from the catalog with `h`.
//! `f` shows every row of a run's stats, as `--full` does, and folds them again.
//! Either scrolls where it is taller than what shows of it. Escape goes back to where
//! it was opened.
//!
//! A run's stats are made at each draw from the run in view, which the state keeps of
//! what it follows, framed at the instant of the draw and costed at the prices the
//! repository's reading said: they move with the run as its news come, and nothing is
//! read for them. Until the run in view is the stage's run, they are being read. A
//! promotion moves the stage onto the successor (`succeeded`), whose stats start at
//! their first line. A workflow's past runs are read once, as the stage opens.

use chrono::{DateTime, Utc};
use surco::components::{Laid, Note, Region, Screen};
use surco::doc::{Depth, Doc};
use surco::look::Look;
use yunta_api::HistoryReading;
use yunta_core::{RunId, WorkflowName};
use yunta_render::page::{History, Page, Stats};
use yunta_render::screen;
use yunta_replay::stats::RunStats;

use super::Want;
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{FollowedRuns, Operation, Outcome};

/// What a run cost, or what a workflow's past runs did.
#[derive(Debug, Clone)]
pub struct StatsStage {
    spending: Spending,
    /// How much of a run's stats stands: the glance, or every row.
    depth: Depth,
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
            read: None,
        })
    }

    fn of(spending: Spending) -> Self {
        StatsStage {
            spending,
            depth: Depth::Glance,
            scroll: Scroll::default(),
            said: None,
        }
    }

    /// What the stage asks for as it opens: a workflow's past runs, read; nothing for a
    /// run's, which the run in view makes.
    pub(crate) fn opening(&self) -> Vec<Want> {
        match &self.spending {
            Spending::Run(_) => Vec::new(),
            Spending::Workflow { workflow, .. } => vec![Want::Work(Operation::ReadHistory {
                workflow: workflow.clone(),
            })],
        }
    }

    /// A run's stats as its command prints them of the run in view `followed` keeps at
    /// `now`, at the prices it keeps, or what was read of a workflow's past runs; or that
    /// either is being read; what refused or failed; with the keys.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Screen {
        let (mut doc, trail, legend) = match &self.spending {
            Spending::Run(run) => {
                let doc = self.run_spent(run, look, followed, now);
                let trail = vec![run.handle().to_string(), screen::STATS.to_string()];
                let full = matches!(self.depth, Depth::Full);
                (doc, trail, screen::stats_legend(full))
            }
            Spending::Workflow { workflow, read } => {
                let doc = match read {
                    Some(past) => History {
                        name: workflow,
                        history: &past.summaries,
                        findings: past.findings.as_ref(),
                    }
                    .document(look),
                    None => Doc::new().with(Note(screen::READING_THE_HISTORY.to_string())),
                };
                let trail = vec![
                    screen::WORKFLOWS.to_string(),
                    workflow.to_string(),
                    screen::HISTORY.to_string(),
                ];
                (doc, trail, screen::over_the_workflows_legend())
            }
        };
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let mut shown = Screen::new(Region::new(doc).in_sight(self.scroll.sight()));
        shown.trail = std::iter::once(screen::YUNTA.to_string())
            .chain(trail)
            .collect();
        shown.legend = match self.scroll.scrolls() {
            true => screen::scrolling(legend),
            false => legend,
        };
        shown
    }

    /// What `run` cost as `yunta stats` prints it, made from the run in view `followed`
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
        let Some(in_view) = in_view.filter(|in_view| in_view.run.run == *run) else {
            return Doc::new().with(Note(screen::READING_THE_STATS.to_string()));
        };
        let kept = &in_view.run;
        Stats {
            frame: &kept.frame(now),
            stats: &RunStats::at(&kept.state, &kept.manifest.workflow, now),
            pricing: followed.prices.as_ref(),
            depth: self.depth,
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

    /// What `stroke` asks for here: `f` shows every row of a run's stats or folds
    /// them, Escape goes back, `q` or Ctrl-C leaves, and the keys that scroll scroll
    /// the page.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        match (stroke, &self.spending) {
            (Stroke::Insert('f'), Spending::Run(_)) => {
                self.depth = match self.depth {
                    Depth::Glance => Depth::Full,
                    Depth::Full | Depth::Dig => Depth::Glance,
                };
                Vec::new()
            }
            (Stroke::Decline, _) => vec![Want::Back],
            (Stroke::Insert('q') | Stroke::Interrupt, _) => vec![Want::Leave],
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

    /// What an answer to the stage's reading does: a workflow's past runs read stand;
    /// what refused or failed is said.
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
                | Outcome::Page(_)
                | Outcome::NodePage(_)
                | Outcome::Questions(_)
                | Outcome::Decision(_)
                | Outcome::Decided { .. }
                | Outcome::Answered { .. }
                | Outcome::Started { .. }
                | Outcome::Resumed { .. }
                | Outcome::Cancelled { .. }
                | Outcome::Closed { .. }
                | Outcome::Detected { .. }
                | Outcome::Initialized(_)
                | Outcome::Stopped,
                _,
            ) => {}
        }
        Vec::new()
    }
}
