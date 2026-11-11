//! See [`super`]. A workflow's graph, as `yunta graph` draws it: a run's, opened from
//! the run with `g`, its workflow as the run froze it and each node labelled with the
//! word its log derives for it; or a workflow's as written, opened from the catalog
//! with `g`. A graph taller than what shows of it scrolls. Escape goes back to where it
//! was opened.

use surco::components::{Laid, Note, Region, Screen};
use surco::doc::Doc;
use surco::look::Look;
use yunta_core::RunId;
use yunta_render::page::{Graph, Page};
use yunta_render::screen;

use super::Want;
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{Operation, Outcome};

/// A workflow's graph, as written or as a run left it.
#[derive(Debug, Clone)]
pub struct GraphStage {
    drawn: Drawn,
    /// The graph, once read.
    graph: Option<Box<Graph>>,
    /// Where the graph is scrolled to.
    scroll: Scroll,
    /// What refused or failed, where something did.
    said: Option<String>,
}

/// Whose graph is drawn: a run's, or a workflow the catalog offers, by the name it
/// offers it under.
#[derive(Debug, Clone)]
enum Drawn {
    Run(RunId),
    Workflow(String),
}

impl GraphStage {
    /// The graph of the workflow `run` froze, yet to be read.
    pub(crate) fn of_run(run: RunId) -> Self {
        GraphStage::of(Drawn::Run(run))
    }

    /// The graph of the workflow the catalog offers as `workflow`, yet to be read.
    pub(crate) fn of_workflow(workflow: String) -> Self {
        GraphStage::of(Drawn::Workflow(workflow))
    }

    fn of(drawn: Drawn) -> Self {
        GraphStage {
            drawn,
            graph: None,
            scroll: Scroll::default(),
            said: None,
        }
    }

    /// What the stage asks for as it opens: the graph, read.
    pub(crate) fn opening(&self) -> Vec<Want> {
        let reading = match &self.drawn {
            Drawn::Run(run) => Operation::ReadRunGraph { run: run.clone() },
            Drawn::Workflow(workflow) => Operation::ReadGraph {
                workflow: workflow.clone(),
            },
        };
        vec![Want::Work(reading)]
    }

    /// The graph as `yunta graph` draws it, or that it is being read; what refused
    /// or failed; with the keys.
    pub(crate) fn screen(&self, look: &Look) -> Screen {
        let mut doc = match &self.graph {
            Some(graph) => graph.document(look),
            None => Doc::new().with(Note(screen::READING_THE_GRAPH.to_string())),
        };
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let (trail, legend) = match &self.drawn {
            Drawn::Run(run) => (
                vec![run.handle().to_string()],
                screen::over_the_run_legend(),
            ),
            Drawn::Workflow(workflow) => (
                vec![screen::WORKFLOWS.to_string(), workflow.clone()],
                screen::over_the_workflows_legend(),
            ),
        };
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

    /// What an answer to the stage's reading does: the graph stands; what refused or
    /// failed is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match outcome {
            Outcome::Graph(graph) => self.graph = Some(graph),
            Outcome::Refused(refused) => self.said = Some(refused.to_string()),
            Outcome::Failed(said) => self.said = Some(said),
            Outcome::Stats(_)
            | Outcome::History(_)
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
            | Outcome::Stopped => {}
        }
        Vec::new()
    }
}
