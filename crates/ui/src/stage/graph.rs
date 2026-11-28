//! See [`super`]. A workflow's graph, each node a box holding its id: a run's, opened
//! from the run with `g`, its workflow as the run froze it, headed by the mode it runs
//! in, the trail naming the workflow, in the run's frame (`over_the_run`), where `a` goes
//! where its attention row names and `n` opens a new run over it; or a
//! workflow's as written, opened from the catalog with `g`, the trail naming the workflow as
//! the catalog offers it, in the frame of the field it was opened over (`over_the_field`),
//! where `a` and `n` do the same. A graph taller than what shows of it scrolls. Escape goes
//! back to where it was opened, and `q` down to the field.
//!
//! A run's graph is made at each draw from the run in view, which the state keeps of what
//! it follows, and nothing is read for it. Until the run in view is the stage's run, it
//! is being read. A promotion moves the stage onto the successor (`succeeded`), whose
//! graph, of the workflow and the mode it was promoted to, starts at its first line. A
//! workflow's graph as written is read once, as the stage opens.

use chrono::{DateTime, Utc};
use surco::components::{Laid, Legend, Note, Region, Screen};
use surco::doc::Doc;
use surco::look::Look;
use yunta_core::step::{Arg, NextStep};
use yunta_core::RunId;
use yunta_render::page::{Graph, Page};
use yunta_render::screen;

use super::over_the_field::{attending_over_the_field, in_the_fields_frame, KeyA};
use super::over_the_run::{attending, in_the_runs_frame};
use super::{ItsRun, Want};
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
    /// What it is to its run, where it draws a run's graph: a view of it as it moves.
    pub(super) fn its_run(&self) -> Option<ItsRun> {
        match &self.drawn {
            Drawn::Run(run) => Some(ItsRun::Shows(run.clone())),
            Drawn::Workflow { .. } => None,
        }
    }

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
            Drawn::Workflow { workflow, .. } => vec![Want::Work(graph_read(workflow))],
        }
    }

    /// The work its screen says is under way: a workflow's graph read, until it is.
    pub(crate) fn shown_under_way(&self) -> Option<Operation> {
        match &self.drawn {
            Drawn::Workflow {
                workflow,
                graph: None,
            } => Some(graph_read(workflow)),
            Drawn::Workflow { graph: Some(_), .. } | Drawn::Run(_) => None,
        }
    }

    /// The graph, a run's of the run in view `followed` keeps at `now`, in the run's frame,
    /// a line typed in what stands over it where `typed`; or a workflow's as written; or that
    /// either is being read; what refused or failed; with the keys.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
        typed: bool,
    ) -> Screen {
        let (mut doc, trail) = match &self.drawn {
            Drawn::Run(run) => of_the_run(run, look, followed),
            Drawn::Workflow { workflow, graph } => {
                let doc = graph
                    .as_ref()
                    .map_or_else(reading, |graph| graph.document(look));
                let trail = vec![
                    screen::WORKFLOWS.to_string(),
                    screen::place_of(workflow, screen::GRAPH),
                ];
                (doc, trail)
            }
        };
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let mut shown = Screen::new(Region::new(doc).in_sight(self.scroll.sight()));
        shown.trail = std::iter::once(screen::YUNTA.to_string())
            .chain(trail)
            .collect();
        shown.legend = screen::page_legend(
            screen::PageRead::Scrolled,
            Legend::new(),
            followed.can_ask_for_a_run(),
        );
        match &self.drawn {
            Drawn::Run(run) => in_the_runs_frame(&mut shown, run, followed, now, typed),
            Drawn::Workflow { .. } => {
                in_the_fields_frame(&mut shown, followed, now, typed, KeyA::Attends);
            }
        }
        shown
    }

    /// Where the graph stands, a run's or a workflow's as written, which no row of its screen
    /// says, its legend the same whether it scrolls or not.
    pub(crate) fn page(&self) -> Scroll {
        self.scroll
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

    /// What `stroke` asks for here, over what `followed` keeps at `now`: `a` goes where the
    /// attention row names and `n` opens a new run over the graph; Escape goes back, `q` goes
    /// down to the field, Ctrl-C leaves, and the keys that scroll scroll the graph.
    pub(crate) fn pressed(
        &mut self,
        stroke: Stroke,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        match (stroke, &self.drawn) {
            (Stroke::Insert('a'), Drawn::Run(run)) => attending(run, followed, now),
            (Stroke::Insert('a'), Drawn::Workflow { .. }) => {
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

    /// What a surface laid out of the graph, which the keys that scroll move from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        self.scroll.laid(laid);
    }

    /// What an answer to the stage's work does: a workflow's graph read stands; what refused
    /// or failed is said.
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

/// What stands while a workflow is read to be drawn.
fn reading() -> Doc {
    Doc::new().with(Note(screen::READING_THE_GRAPH.to_string()))
}

/// The graph of the workflow `run` froze, headed by the mode it runs in, made from the run in
/// view `followed` keeps once it is `run`, else that it is being read; and the trail's places
/// after Yunta's, the workflow's name and the graph's word, the run's handle in the name's
/// place until it is read.
fn of_the_run(run: &RunId, look: &Look, followed: &FollowedRuns) -> (Doc, Vec<String>) {
    let in_view = followed.in_view.as_deref();
    let Some(in_view) = in_view.filter(|in_view| in_view.run.run() == run) else {
        let reading = Note(screen::READING_THE_RUNS_GRAPH.to_string());
        return (
            Doc::new().with(reading),
            vec![screen::place_of(run.handle(), screen::GRAPH)],
        );
    };
    let workflow = &in_view.run.manifest().workflow;
    let graph = Graph {
        workflow: workflow.clone(),
        mode: Some(in_view.run.state().run.mode().clone()),
    };
    let place = screen::place_of(workflow.name.as_str(), screen::GRAPH);
    (graph.document(look), vec![place])
}

/// The work that reads the workflow the catalog offers as `workflow`, to draw it.
fn graph_read(workflow: &str) -> Operation {
    Operation::ReadGraph {
        workflow: workflow.to_string(),
    }
}
