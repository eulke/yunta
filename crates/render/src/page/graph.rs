//! A workflow's graph as a person reads it on a page: a headline naming the
//! workflow, and the run when the graph is a run's, over the diagram of its nodes.
//! A run's graph labels each node with what the run's log derives for it, so the
//! diagram and the run's page say the same word about the same node.

use surco::components::Headline;
use surco::diagram::Diagram;
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::{RunId, Workflow};
use yunta_replay::frame::RunFrame;

use super::Page;
use crate::graph::{self, Labels};

/// What a workflow's graph is drawn from: the workflow, and the run it was drawn
/// from, with the frame its log derives, when it is a run's.
pub struct Graph {
    pub workflow: Workflow,
    pub run: Option<(RunId, RunFrame)>,
}

impl Page for Graph {
    fn document(&self, _look: &Look) -> Doc {
        let named = format!("workflow {}", self.workflow.name);
        let (headline, labels) = match &self.run {
            Some((run, frame)) => (
                Headline::new(
                    frame.standing.mark(),
                    format!("{named} of run {}", run.handle()),
                    frame.standing.word(),
                ),
                Some(Labels::of(frame)),
            ),
            None => (Headline::new(Mark::Absent, named, "as written"), None),
        };
        let source = graph::mermaid(&self.workflow, labels.as_ref());
        Doc::new()
            .with(headline)
            .with(Block::Diagram(Diagram::of(&source)))
    }
}

#[cfg(test)]
mod tests;
