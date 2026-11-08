//! `yunta status <run> --node <id>`: one node whole — where it stands,
//! its whole failure or the whole of what its agent said, the end of what
//! it printed and where all of it is kept, and the documents it produced.
//! What every other surface cuts to a line, this one does not.

use std::path::Path;

use surco::components::{Fields, Headline, Next, Prose, Whole};
use surco::doc::{Block, Doc};
use surco::figures::duration;
use surco::look::Look;
use surco::paths;
use yunta_core::events::NodeState;
use yunta_core::step::NextStep;
use yunta_core::{NodeId, RunId};
use yunta_render::blocks::FailureDetail;
use yunta_render::command;
use yunta_render::spend::Breakdown;
use yunta_replay::frame::{NodeFrame, NodeStanding, RunFrame};
use yunta_replay::RunState;

use crate::error::CliError;

/// What the page is drawn from.
pub(super) struct NodePage<'a> {
    pub(super) run_id: &'a RunId,
    /// The node the page is about, as the run's frame derives it.
    pub(super) node: &'a NodeFrame,
    pub(super) run_dir: &'a Path,
    /// Where paths are shown from: the directory this was run in, and
    /// the home `~` stands for.
    pub(super) cwd: &'a Path,
    pub(super) home: Option<&'a Path>,
    /// The documents the node produced, each read whole here.
    pub(super) documents: &'a [yunta_core::shown::ShownDocument],
    pub(super) state: &'a RunState,
}

/// The node `id` names in `frame`. A name the run does not declare is
/// refused with what the run calls the node it meant, when it has one.
pub(super) fn find<'a>(
    run_id: &RunId,
    frame: &'a RunFrame,
    id: &NodeId,
) -> Result<&'a NodeFrame, CliError> {
    frame
        .nodes
        .iter()
        .find(|node| node.id == *id)
        .ok_or_else(|| {
            let declared = frame.nodes.iter().map(|node| node.id.as_str());
            CliError::msg(format!(
                "run {} has no node `{id}`{}",
                run_id.handle(),
                surco::text::did_you_mean(id.as_str(), declared)
            ))
        })
}

impl NodePage<'_> {
    /// The page for the node, as one document: where it stands, what it
    /// said or what failed it, its failure whole, what it took, the
    /// documents it produced, and what to type next.
    pub(super) fn document(&self, _look: &Look) -> Doc {
        let display = crate::render::node_display(self.node);
        let headline = Headline::new(
            display.standing.mark(),
            format!("node {} of run {}", self.node.id, self.run_id.handle()),
            display.standing.word(),
        );
        let documents = self.documents.iter().flat_map(|document| {
            yunta_render::shown::document(document, self.run_id, yunta_render::shown::Form::Whole)
                .into_blocks()
        });
        Doc::new()
            .with(headline)
            .with_all(self.said())
            .with_all(self.evidence())
            .with(self.fields())
            .with_all(documents)
            .with(self.next())
    }

    /// The whole of what the node's state says: its failure, every line
    /// of it, or what its agent said when it finished, as it wrote it.
    fn said(&self) -> Vec<Block> {
        match &self.node.state {
            NodeStanding::Reached(NodeState::Failed { failure, .. }) => {
                vec![Prose(yunta_render::failure::headline(failure)).into()]
            }
            NodeStanding::Reached(NodeState::Finished { outcome, .. }) => {
                vec![Block::Markdown(yunta_render::outcome::sentence(outcome))]
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
                crate::run_page::handed(self.state, &node.id).unwrap_or_default(),
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
