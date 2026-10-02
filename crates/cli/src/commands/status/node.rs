//! `yunta status <run> --node <id>`: one node whole — where it stands,
//! its whole failure or the whole of what its agent said, the end of what
//! it printed and where all of it is kept, and the documents it produced.
//! What every other surface cuts to a line, this one does not.

use std::path::Path;

use yunta_core::events::NodeState;
use yunta_core::{NodeId, RunId};
use yunta_engine::{NodeFrame, NodeStanding, RunFrame};

use crate::commands::advice;
use crate::error::CliError;
use crate::render::blocks::{paint, Block, FailureDetail, Fields, Headline, Next, Whole};
use crate::render::ink::{Line, Tone};
use crate::render::{duration, paths, wrap, Look, NodeDisplay, Tokens, INDENT};

/// What the page is drawn from.
pub(super) struct NodePage<'a> {
    pub(super) run_id: &'a RunId,
    pub(super) frame: &'a RunFrame,
    pub(super) run_dir: &'a Path,
    /// Where paths are shown from: the directory this was run in, and
    /// the home `~` stands for.
    pub(super) cwd: &'a Path,
    pub(super) home: Option<&'a Path>,
}

impl NodePage<'_> {
    /// The page for `id`, or what the run calls the node it meant when
    /// it declares none by that name.
    pub(super) fn render(&self, id: &NodeId, look: &Look) -> Result<String, CliError> {
        let Some(node) = self.frame.nodes.iter().find(|node| node.id == *id) else {
            let declared = self.frame.nodes.iter().map(|node| node.id.as_str());
            return Err(CliError::msg(format!(
                "run {} has no node `{id}`{}",
                self.run_id.handle(),
                yunta_core::text::did_you_mean(id.as_str(), declared)
            )));
        };
        let display = NodeDisplay::standing(&node.state);
        let headline = Headline {
            subject: format!("node {id} of run {}", self.run_id.handle()),
            mark: display.word.mark(),
            said: display.word.word().to_string(),
        };
        let mut out = paint(&[&headline], look);
        for part in [
            self.said(node, look),
            self.evidence(node, look),
            self.fields(node).lines(look),
            self.next().lines(look),
        ] {
            if !part.is_empty() {
                out.push('\n');
                for line in &part {
                    out.push_str(&format!("{}\n", look.ink.paint(line)));
                }
            }
        }
        Ok(out)
    }

    /// The whole of what the node's state says: its failure, every line
    /// of it, or what its agent said when it finished.
    fn said(&self, node: &NodeFrame, look: &Look) -> Vec<Line> {
        let (tone, text) = match &node.state {
            NodeStanding::Reached(NodeState::Failed { failure, .. }) => {
                (Tone::Failed, failure.headline())
            }
            NodeStanding::Reached(NodeState::Finished { outcome, .. }) => {
                (Tone::Plain, outcome.clone())
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
    fn evidence(&self, node: &NodeFrame, look: &Look) -> Vec<Line> {
        let NodeStanding::Reached(NodeState::Failed { failure, .. }) = &node.state else {
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
    fn fields(&self, node: &NodeFrame) -> Fields {
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
            _ => format!(
                "{} in / {} out",
                Tokens(node.tokens.input).figure(),
                Tokens(node.tokens.output).figure()
            ),
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
            .push_if("produced", produced.join(", "))
    }

    /// Where to read the rest of the run.
    fn next(&self) -> Next {
        Next {
            steps: vec![(advice::status(self.run_id.handle()), "shows the whole run")],
        }
    }
}
