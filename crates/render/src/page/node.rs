//! `yunta status <run> --node <id>`: one node whole — where it stands,
//! its whole failure or the whole of what its agent said, the end of what
//! it printed and where all of it is kept, and the documents it produced.
//! What every other surface cuts to a line, this one does not.

use std::path::Path;

use super::{Form, Page, Shown};
use crate::blocks::FailureDetail;
use crate::command;
use crate::spend::Breakdown;
use surco::components::{Fields, Headline, Next, Prose, Whole};
use surco::doc::{Block, Doc};
use surco::figures::duration;
use surco::look::Look;
use surco::paths;
use yunta_core::events::NodeState;
use yunta_core::step::NextStep;
use yunta_core::RunId;
use yunta_replay::frame::{NodeFrame, NodeStanding};
use yunta_replay::RunState;

/// A node of a run, as `status --node` shows it: where it stands, what it
/// said or what failed it, what it took, the documents it produced, and what
/// to type next.
pub struct Node<'a> {
    pub run_id: &'a RunId,
    /// The node the page is about, as the run's frame derives it.
    pub node: &'a NodeFrame,
    pub run_dir: &'a Path,
    /// Where paths are shown from: the directory this was run in, and
    /// the home `~` stands for.
    pub cwd: &'a Path,
    pub home: Option<&'a Path>,
    /// The documents the node produced, each read whole here.
    pub documents: &'a [yunta_core::shown::ShownDocument],
    pub state: &'a RunState,
}

impl Page for Node<'_> {
    /// The page for the node, as one document: where it stands, what it
    /// said or what failed it, its failure whole, what it took, the
    /// documents it produced, and what to type next.
    fn document(&self, look: &Look) -> Doc {
        let display = crate::state::NodeDisplay::from(self.node);
        let headline = Headline::new(
            display.standing.mark(),
            format!("node {} of run {}", self.node.id, self.run_id.handle()),
            display.standing.word(),
        );
        let documents = self.documents.iter().flat_map(|document| {
            let shown = Shown {
                document,
                run: self.run_id,
                form: Form::Whole,
            };
            shown.document(look).into_blocks()
        });
        Doc::new()
            .with(headline)
            .with_all(self.said())
            .with_all(self.evidence())
            .with(self.fields())
            .with_all(documents)
            .with(self.next())
    }
}

impl Node<'_> {
    /// The whole of what the node's state says: its failure, every line
    /// of it, or what its agent said when it finished, as it wrote it.
    fn said(&self) -> Vec<Block> {
        match &self.node.state {
            NodeStanding::Reached(NodeState::Failed { failure, .. }) => {
                vec![Prose(crate::failure::headline(failure)).into()]
            }
            NodeStanding::Reached(NodeState::Finished { outcome, .. }) => {
                vec![Block::Markdown(crate::outcome::sentence(outcome))]
            }
            _ => Vec::new(),
        }
    }

    /// The end of what the node printed and where the whole of it is
    /// kept, each document it refused, the paths it should not have
    /// touched.
    fn evidence(&self) -> Vec<Block> {
        let NodeStanding::Reached(NodeState::Failed { failure, .. }) = &self.node.state else {
            return Vec::new();
        };
        let whole = failure.output().map(|output| {
            Whole::file(
                yunta_core::run_dir::object(self.run_dir, output),
                self.cwd,
                self.home,
            )
        });
        FailureDetail { failure, whole }.blocks()
    }

    /// What the node is and what it took.
    fn fields(&self) -> Fields {
        let node = self.node;
        let runner = node
            .runner
            .as_ref()
            .map(|runner| {
                format!(
                    "{} on {}/{}",
                    runner.runner, runner.chosen.adapter, runner.chosen.model
                )
            })
            .unwrap_or_default();
        let tokens = match node.tokens.total() {
            0 => String::new(),
            _ => Breakdown(&node.tokens).to_string(),
        };
        let produced: Vec<String> = node
            .artifacts
            .iter()
            .map(|artifact| {
                paths::shown(
                    &self.run_dir.join(yunta_core::run_dir::view(
                        Some(&node.id),
                        &artifact.view_name(),
                    )),
                    self.cwd,
                    self.home,
                )
            })
            .collect();
        Fields::new()
            .push_if("kind", node.kind)
            .push_if(
                "attempt",
                node.attempt.map(|n| n.to_string()).unwrap_or_default(),
            )
            .push_if("runner", runner)
            .push_if("tokens", tokens)
            .push_if("took", node.elapsed.map(duration).unwrap_or_default())
            .push_if(
                "handed over",
                crate::state::handed(self.state, &node.id).unwrap_or_default(),
            )
            .push_if("produced", produced.join(", "))
    }

    /// Where to read the rest of the run.
    fn next(&self) -> Next {
        Next {
            steps: vec![(
                command::line(&NextStep::status(self.run_id)),
                "shows the whole run",
            )],
        }
    }
}
