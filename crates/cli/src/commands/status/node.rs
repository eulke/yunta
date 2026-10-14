//! `yunta status <run> --node <id>`: one node whole — where it stands,
//! its whole failure or the whole of what its agent said, the end of what
//! it printed and where all of it is kept, and the documents it produced.
//! What every other surface cuts to a line, this one does not.

use std::path::Path;

use yunta_core::events::NodeState;
use yunta_core::{NodeId, RunId};
use yunta_engine::{NodeFrame, NodeStanding, RunFrame, RunState};
use yunta_render::spend::Breakdown;

use crate::commands::advice;
use crate::error::CliError;
use crate::render::blocks::{Drawn, FailureDetail, Fields, Headline, Next, Whole};
use crate::render::doc::Doc;
use crate::render::ink::{Line, Tone};
use crate::render::{duration, paths, wrap, Look, INDENT};
use crate::run_page::section;

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
                yunta_core::text::did_you_mean(id.as_str(), declared)
            ))
        })
}

impl NodePage<'_> {
    /// The page for the node, as one document: where it stands, what it
    /// said or what failed it, its failure whole, what it took, the
    /// documents it produced, and what to type next.
    pub(super) fn document(&self, look: &Look) -> Doc<'_> {
        let display = crate::render::standing(&self.node.state);
        let headline = Headline {
            subject: format!("node {} of run {}", self.node.id, self.run_id.handle()),
            mark: display.word.mark(),
            said: display.word.word().to_string(),
        };
        let mut documents = Vec::new();
        for document in self.documents {
            let doc = crate::render::shown::document(
                document,
                self.run_id,
                crate::render::shown::Form::Whole,
                &advice::spell,
            );
            documents.extend(crate::render::surface_lines(doc, look));
        }
        let doc = section(Doc::new().with(headline), self.said(look));
        let doc = section(doc, self.evidence(look));
        let doc = section(doc, self.fields().lines(look));
        let doc = section(doc, documents);
        section(doc, self.next().lines(look))
    }

    /// The whole of what the node's state says: its failure, every line
    /// of it, or what its agent said when it finished.
    fn said(&self, look: &Look) -> Vec<Line> {
        let (tone, text) = match &self.node.state {
            NodeStanding::Reached(NodeState::Failed { failure, .. }) => {
                (Tone::Failed, crate::render::failure::headline(failure))
            }
            NodeStanding::Reached(NodeState::Finished { outcome, .. }) => {
                (Tone::Plain, yunta_render::outcome::sentence(outcome))
            }
            _ => return Vec::new(),
        };
        let room = look.width.cells().saturating_sub(INDENT.len());
        text.lines()
            .flat_map(|line| match line.trim().is_empty() {
                true => vec![String::new()],
                false => wrap(line, room),
            })
            .map(|line| match line.is_empty() {
                true => Line::new(),
                false => Line::new().plain(INDENT).push(tone, line),
            })
            .collect()
    }

    /// The end of what the node printed and where the whole of it is
    /// kept, each document it refused, the paths it should not have
    /// touched.
    fn evidence(&self, look: &Look) -> Vec<Line> {
        let NodeStanding::Reached(NodeState::Failed { failure, .. }) = &self.node.state else {
            return Vec::new();
        };
        let whole = failure.output().map(|output| {
            Whole::file(
                yunta_engine::ObjectStore::at(self.run_dir).path_of(output),
                self.cwd,
                self.home,
            )
        });
        FailureDetail { failure, whole }.lines(look)
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
                    &self.run_dir.join(yunta_engine::view_path(
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
                super::page::handed(self.state, &node.id).unwrap_or_default(),
            )
            .push_if("produced", produced.join(", "))
    }

    /// Where to read the rest of the run.
    fn next(&self) -> Next {
        Next {
            steps: vec![(advice::status(self.run_id.handle()), "shows the whole run")],
        }
    }
}
