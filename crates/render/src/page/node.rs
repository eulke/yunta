//! `yunta status <run> --node <id>`: one node whole — where it stands,
//! its whole failure or the whole of what its agent said, the end of what
//! it printed and where all of it is kept, and the documents it produced.
//! What every other surface cuts to a line, this one does not.
//!
//! The node says in print what its pane shows, in the pane's parts and
//! words: what it hands over, each task's proof and scope where it is a
//! loop, and its feed under its heading. Those parts stand as the pane
//! draws them, a heading and its rows at one column, so a row of the page
//! reads as the same row of the pane; the page's own facts keep the
//! page's indent. The feed shows its newest rows, and how many stand
//! above them. A document the node handed over that cannot be read here
//! is said in its place, in its own errors, and the rest of the page
//! prints.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{Form, Page, Shown};
use crate::blocks::{FailureDetail, HandsOver, TaskProof, TaskScope};
use crate::chronicle::{feed, feed_heading, in_feed};
use crate::command;
use crate::spend::Breakdown;
use surco::components::{Fields, Headline, Next, Prose, Whole};
use surco::doc::{Block, Doc};
use surco::figures::duration;
use surco::look::Look;
use surco::paths;
use yunta_core::events::{node, session, ArtifactId, NodeState};
use yunta_core::step::NextStep;
use yunta_core::{ArtifactSpec, RunId, TaskId};
use yunta_replay::frame::{Happening, Moment, NodeFrame, NodeStanding};
use yunta_replay::RunState;

/// How many of a node's newest feed rows its page prints: what reads at once under
/// the parts above it, a 24-row terminal less the feed's heading, the rows that count
/// what stands above and the next steps under it.
const FEED_ROWS: usize = 20;

/// A node of a run, as `status --node` shows it: where it stands, what it
/// said or what failed it, what it took, the documents it produced, and what
/// to type next.
#[derive(Debug, Clone)]
pub struct Node {
    pub run_id: RunId,
    /// The node the page is about, as the run's frame derives it.
    pub node: NodeFrame,
    pub run_dir: PathBuf,
    /// Where paths are shown from: the directory this was run in, and
    /// the home `~` stands for.
    pub cwd: PathBuf,
    pub home: Option<PathBuf>,
    /// The documents the node produced, each read whole here.
    pub documents: Vec<yunta_core::shown::ShownDocument>,
    /// Each document the node handed over that cannot be read here, with why, as its
    /// reader worded it.
    pub unread: Vec<(ArtifactId, String)>,
    pub state: RunState,
    /// What the node's workflow declares it produces, in the order declared.
    pub declared: Vec<ArtifactSpec>,
    /// For a loop, each task it works, in its tasks document's order, with the title
    /// the document gives it; none for any other node.
    pub tasks: Vec<(TaskId, Option<String>)>,
    /// What happened at the node, in the log's order: its moments of the run's
    /// chronicle.
    pub moments: Vec<Moment>,
}

impl Page for Node {
    /// The page for the node, as one document: where it stands, what it
    /// said or what failed it, its failure whole, what it took, what it
    /// hands over, each task's proof and scope where it is a loop, its
    /// feed's newest rows, the documents it produced, and what to type next.
    fn document(&self, look: &Look) -> Doc {
        let display = crate::state::NodeDisplay::from(&self.node);
        let headline = Headline::new(
            display.standing.mark(),
            format!("node {} of run {}", self.node.id, self.run_id.handle()),
            display.standing.word(),
        );
        let documents = self.documents.iter().flat_map(|document| {
            let shown = Shown {
                document,
                run: &self.run_id,
                form: Form::Whole,
            };
            shown.document(look).into_blocks()
        });
        let handed = HandsOver::of(&self.node.id, &self.declared, &self.state, &self.unread);
        Doc::new()
            .with(headline)
            .with_all(self.said())
            .with_all(self.evidence())
            .with(self.fields())
            .with_all(handed.blocks(look))
            .with_all(self.tasks(look))
            .with_all(self.feed(look))
            .with_all(documents)
            .with(self.next())
    }
}

impl Node {
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
                yunta_core::run_dir::object(&self.run_dir, output),
                &self.cwd,
                self.home.as_deref(),
            )
        });
        FailureDetail { failure, whole }.blocks()
    }

    /// What the node is and what it took.
    fn fields(&self) -> Fields {
        let node = &self.node;
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
                    &self.cwd,
                    self.home.as_deref(),
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
                crate::state::handed(&self.state, &node.id).unwrap_or_default(),
            )
            .push_if("produced", produced.join(", "))
    }

    /// Each task the loop works, its own row and how it is proved, then
    /// its scope.
    fn tasks(&self, look: &Look) -> Vec<Block> {
        self.tasks
            .iter()
            .flat_map(|(task, title)| {
                let proof = TaskProof::of(task, title.as_deref(), &self.state);
                let scope = TaskScope::of(task, &self.state);
                let proof = proof.map(|proof| proof.blocks(look.glyphs));
                let scope = scope.map(|scope| scope.blocks(look.glyphs));
                proof.into_iter().chain(scope).flatten()
            })
            .collect()
    }

    /// What the node is doing under its heading: the newest of the moments
    /// its feed lists, and how many stand above them. Nothing for a node
    /// with no moment the feed lists.
    fn feed(&self, look: &Look) -> Vec<Block> {
        let listed: Vec<Moment> = self
            .moments
            .iter()
            .filter(|moment| in_feed(&moment.happening))
            .cloned()
            .collect();
        if listed.is_empty() {
            return Vec::new();
        }
        let above = listed.len().saturating_sub(FEED_ROWS);
        let none = BTreeMap::new();
        let tasks = self
            .state
            .nodes
            .get(&self.node.id)
            .map_or(&none, |record| &record.session_tasks);
        let fed = feed(&listed[above..], tasks, above, 0, look);
        vec![
            Block::Heading(feed_heading(self.calls(), look.glyphs)),
            fed.into(),
        ]
    }

    /// How many calls the node's latest attempt made: those after the
    /// moment it started, open or closed since.
    fn calls(&self) -> usize {
        let started = self.moments.iter().rposition(|moment| {
            matches!(
                moment.happening,
                Happening::Node(node::happening::Happening::Reached {
                    state: NodeState::Running { .. },
                    ..
                })
            )
        });
        self.moments[started.map_or(0, |at| at + 1)..]
            .iter()
            .filter(|moment| {
                matches!(
                    moment.happening,
                    Happening::Session(session::happening::Happening::Called { .. })
                )
            })
            .count()
    }

    /// Where to read the rest of the run.
    fn next(&self) -> Next {
        Next {
            steps: vec![(
                command::line(&NextStep::status(&self.run_id)),
                "shows the whole run",
            )],
        }
    }
}

#[cfg(test)]
mod tests;
