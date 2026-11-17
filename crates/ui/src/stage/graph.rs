//! See [`super`]. A workflow's graph, as `yunta graph` draws it: a run's, opened from
//! the run with `g`, its workflow as the run froze it and each node labelled with the
//! word its log derives for it; or a workflow's as written, opened from the catalog
//! with `g`. A graph taller than what shows of it scrolls. Escape goes back to where it
//! was opened.
//!
//! A run's graph is made at each draw from the run in view, which the state keeps of what
//! it follows, framed at the instant of the draw: each node's word moves with the run as
//! its news come, and nothing is read for it. Until the run in view is the stage's run,
//! it is being read. A promotion moves the stage onto the successor (`succeeded`), whose
//! graph starts at its first line. A workflow's graph as written is read once, as the
//! stage opens.

use chrono::{DateTime, Utc};
use surco::components::{Laid, Note, Region, Screen};
use surco::doc::Doc;
use surco::look::Look;
use yunta_core::RunId;
use yunta_render::page::{Graph, Page};
use yunta_render::screen;

use super::Want;
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{FollowedRuns, Operation, Outcome};

/// A workflow's graph, as written or as a run left it.
#[derive(Debug, Clone)]
pub struct GraphStage {
    drawn: Drawn,
    /// Where the graph is scrolled to.
    scroll: Scroll,
    /// What refused or failed, where something did.
    said: Option<String>,
}

/// Whose graph is drawn: a run's, made from the run in view at each draw, or a workflow
/// the catalog offers, by the name it offers it under, with its graph once read.
#[derive(Debug, Clone)]
enum Drawn {
    Run(RunId),
    Workflow {
        workflow: String,
        graph: Option<Box<Graph>>,
    },
}

impl GraphStage {
    /// The graph of the workflow `run` froze, made from the run in view once it is `run`.
    pub(crate) fn of_run(run: RunId) -> Self {
        GraphStage::of(Drawn::Run(run))
    }

    /// The graph of the workflow the catalog offers as `workflow`, yet to be read.
    pub(crate) fn of_workflow(workflow: String) -> Self {
        GraphStage::of(Drawn::Workflow {
            workflow,
            graph: None,
        })
    }

    fn of(drawn: Drawn) -> Self {
        GraphStage {
            drawn,
            scroll: Scroll::default(),
            said: None,
        }
    }

    /// What the stage asks for as it opens: a workflow's graph, read; nothing for a run's,
    /// which the run in view makes.
    pub(crate) fn opening(&self) -> Vec<Want> {
        match &self.drawn {
            Drawn::Run(_) => Vec::new(),
            Drawn::Workflow { workflow, .. } => vec![Want::Work(Operation::ReadGraph {
                workflow: workflow.clone(),
            })],
        }
    }

    /// The graph as `yunta graph` draws it, a run's of the run in view `followed` keeps
    /// at `now`, or that it is being read; what refused or failed; with the keys.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Screen {
        let reading = || Doc::new().with(Note(screen::READING_THE_GRAPH.to_string()));
        let (mut doc, trail, legend) = match &self.drawn {
            Drawn::Run(run) => {
                let in_view = followed
                    .in_view
                    .as_deref()
                    .filter(|in_view| in_view.run.run == *run);
                let doc = match in_view {
                    Some(in_view) => Graph {
                        workflow: in_view.run.manifest.workflow.clone(),
                        run: Some((run.clone(), in_view.run.frame(now))),
                    }
                    .document(look),
                    None => reading(),
                };
                let trail = vec![run.handle().to_string()];
                (doc, trail, screen::over_the_run_legend())
            }
            Drawn::Workflow { workflow, graph } => {
                let doc = match graph {
                    Some(graph) => graph.document(look),
                    None => reading(),
                };
                let trail = vec![screen::WORKFLOWS.to_string(), workflow.clone()];
                (doc, trail, screen::over_the_workflows_legend())
            }
        };
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let mut shown = Screen::new(Region::new(doc).in_sight(self.scroll.sight()));
        shown.trail = std::iter::once(screen::YUNTA.to_string())
            .chain(trail)
            .chain(std::iter::once(screen::GRAPH.to_string()))
            .collect();
        shown.legend = match self.scroll.scrolls() {
            true => screen::scrolling(legend),
            false => legend,
        };
        shown
    }

    /// The stage, where it shows `run`'s graph, moved onto `by`, the successor a promotion
    /// handed the work to: its graph, from its first line, once the run in view is `by`.
    pub(crate) fn succeeded(&mut self, run: &RunId, by: &RunId) {
        if let Drawn::Run(shown) = &mut self.drawn {
            if shown == run {
                *shown = by.clone();
                self.scroll.rewound();
            }
        }
    }

    /// What `stroke` asks for here: Escape goes back, `q` or Ctrl-C leaves, and the
    /// keys that scroll scroll the graph.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        match stroke {
            Stroke::Decline => vec![Want::Back],
            Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
            other => {
                self.scroll.pressed(other);
                Vec::new()
            }
        }
    }

    /// What a surface laid out of the graph, which the keys that scroll move from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        self.scroll.laid(laid);
    }

    /// What an answer to the stage's reading does: a workflow's graph read stands; what
    /// refused or failed is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match (outcome, &mut self.drawn) {
            (Outcome::Graph(read), Drawn::Workflow { graph, .. }) => *graph = Some(read),
            (Outcome::Refused(refused), _) => self.said = Some(refused.to_string()),
            (Outcome::Failed(said), _) => self.said = Some(said),
            (
                Outcome::Graph(_)
                | Outcome::History(_)
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
