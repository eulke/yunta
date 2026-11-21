//! A workflow's graph as a person reads it on a page: a headline naming the
//! workflow and what the graph shows, the nodes it runs in order, with the
//! run's mode at its far end when the graph is a run's, or `as written`; over
//! the diagram of its nodes, each a box that holds its id alone. What a run's
//! log derives for each node is its page's and its stats' to say, and the
//! Mermaid and DOT `yunta graph --run` prints write it inside each node.

use surco::components::Headline;
use surco::diagram::Diagram;
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::{ModeName, Workflow};

use super::Page;
use crate::graph;

/// What a workflow's graph is drawn from: the workflow, and the mode of the run
/// it is drawn for when it is a run's.
#[derive(Debug, Clone)]
pub struct Graph {
    pub workflow: Workflow,
    /// The mode the run it is drawn for was created in, which its headline names
    /// at its far end; `None` for the workflow as written.
    pub mode: Option<ModeName>,
}

impl Page for Graph {
    fn document(&self, _look: &Look) -> Doc {
        let (mark, edge) = match &self.mode {
            Some(mode) => (Mark::Running, mode.to_string()),
            None => (Mark::Absent, "as written".to_string()),
        };
        let headline = Headline::new(
            mark,
            self.workflow.name.to_string(),
            "the nodes it runs, in order",
        )
        .edge(edge);
        let source = graph::drawn(&self.workflow);
        Doc::new()
            .with(headline)
            .with(Block::Diagram(Diagram::of(&source)))
    }
}

#[cfg(test)]
mod tests;
