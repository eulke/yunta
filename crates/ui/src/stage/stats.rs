//! See [`super`]. What a run cost, as `yunta stats` shows it, opened from the run
//! with `s`; or what a workflow's past runs cost and what their checks never
//! caught, as `yunta stats --workflow` shows it, opened from the catalog with `h`.
//! `f` shows every row of a run's stats, as `--full` does, and folds them again.
//! Either scrolls where it is taller than what shows of it. Escape goes back to where
//! it was opened.

use surco::components::{Laid, Note, Region, Screen};
use surco::doc::{Depth, Doc};
use surco::look::Look;
use yunta_api::{HistoryReading, StatsReading};
use yunta_core::{RunId, WorkflowName};
use yunta_render::page::{History, Page, Stats};
use yunta_render::screen;

use super::Want;
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{Operation, Outcome};

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

/// Whose spending stands: one run's, or every past run of a workflow's; each with
/// what was read of it, once it is.
#[derive(Debug, Clone)]
enum Spending {
    Run {
        run: RunId,
        read: Option<Box<StatsReading>>,
    },
    Workflow {
        workflow: WorkflowName,
        read: Option<Box<HistoryReading>>,
    },
}

impl StatsStage {
    /// What `run` cost, yet to be read.
    pub(crate) fn of_run(run: RunId) -> Self {
        StatsStage::of(Spending::Run { run, read: None })
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

    /// What the stage asks for as it opens: what it shows, read.
    pub(crate) fn opening(&self) -> Vec<Want> {
        let reading = match &self.spending {
            Spending::Run { run, .. } => Operation::ReadStats { run: run.clone() },
            Spending::Workflow { workflow, .. } => Operation::ReadHistory {
                workflow: workflow.clone(),
            },
        };
        vec![Want::Work(reading)]
    }

    /// What was read, as its command prints it, or that it is being read; what
    /// refused or failed; with the keys.
    pub(crate) fn screen(&self, look: &Look) -> Screen {
        let (mut doc, trail, legend) = match &self.spending {
            Spending::Run { run, read } => {
                let doc = match read {
                    Some(spent) => Stats {
                        frame: &spent.frame,
                        stats: &spent.stats,
                        pricing: spent.pricing.as_ref(),
                        depth: self.depth,
                    }
                    .document(look),
                    None => Doc::new().with(Note(screen::READING_THE_STATS.to_string())),
                };
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

    /// What `stroke` asks for here: `f` shows every row of a run's stats or folds
    /// them, Escape goes back, `q` or Ctrl-C leaves, and the keys that scroll scroll
    /// the page.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        match (stroke, &self.spending) {
            (Stroke::Insert('f'), Spending::Run { .. }) => {
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

    /// What an answer to the stage's reading does: what was read stands; what
    /// refused or failed is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match (outcome, &mut self.spending) {
            (Outcome::Stats(spent), Spending::Run { read, .. }) => *read = Some(spent),
            (Outcome::History(past), Spending::Workflow { read, .. }) => *read = Some(past),
            (Outcome::Refused(refused), _) => self.said = Some(refused.to_string()),
            (Outcome::Failed(said), _) => self.said = Some(said),
            (
                Outcome::Stats(_)
                | Outcome::History(_)
                | Outcome::Graph(_)
                | Outcome::Doctor(_)
                | Outcome::Collected(_)
                | Outcome::Repository(_)
                | Outcome::Catalog(_)
                | Outcome::Workflow(_)
                | Outcome::Pack(_)
                | Outcome::Evidence(_)
                | Outcome::Field { .. }
                | Outcome::Followed
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
