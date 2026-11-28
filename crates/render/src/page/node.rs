//! `yunta status <run> --node <id>`: one node whole — where it stands,
//! its whole failure or the whole of what its agent said, the end of what
//! it printed and where all of it is kept, and the documents it produced.
//! What every other surface cuts to a line, this one does not.
//!
//! The node says in print what its pane shows, in the pane's parts and
//! words: how it stands as its own scene says it (that it has not run, why
//! its run leaves it out, or the word it stands at, what it took and spent
//! and what it came to), what it hands over, each task's proof and scope
//! where it is a loop, and its feed under its heading. Those parts stand as
//! the pane draws them, a heading and its rows at one column, so a row of
//! the page reads as the same row of the pane; the page's own facts keep
//! the page's indent, after the scene's. The feed shows its newest rows,
//! and how many stand above them. A document the node handed over that
//! cannot be read here is said in its place, in its own errors, and the
//! rest of the page prints.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{Form, Page, Shown};
use crate::blocks::{runner_on, FailureDetail, HandsOver, HowItStands, TaskProof, TaskScope};
use crate::chronicle::{feed, feed_heading, in_feed, open_attempt_calls};
use crate::command::{self, SaidStep};
use crate::watched::listed_as;
use surco::components::{Fields, Headline, Next, Whole};
use surco::doc::{Block, Doc};
use surco::look::Look;
use surco::paths;
use yunta_core::events::{ArtifactId, NodeState, ResolvedRunner};
use yunta_core::step::NextStep;
use yunta_core::{ArtifactSpec, RunId, TaskId};
use yunta_replay::frame::{Moment, NodeFrame, NodeStanding, NodeTimes};
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
    /// What the clock adds to the node at the instant the page is read at.
    pub times: NodeTimes,
    /// Who works it, as its pane's title names them
    /// ([`runner_of`](crate::blocks::runner_of)): the runner it resolved,
    /// or the one its run resolved for the role it names. Not
    /// [`NodeFrame::runner`], the one the node resolved itself.
    pub runner: Option<ResolvedRunner>,
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
        let standing = listed_as(&self.node);
        let headline = Headline::new(
            standing.mark(),
            format!("node {} of run {}", self.node.id, self.run_id.handle()),
            standing.word(),
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
            .with(self.fields(look))
            .with_all(handed.blocks(look))
            .with_all(self.tasks(look))
            .with_all(self.feed(look))
            .with_all(documents)
            .with(self.next())
    }
}

impl Node {
    /// How the node stands, as its own scene says it: a page waits on no
    /// person, so what a decision holds it on is its failure's own words.
    fn stands(&self) -> HowItStands {
        HowItStands::of(&self.node, &self.times, self.state.run.mode(), false)
    }

    /// The whole of what the node's state says, as its own scene says it:
    /// that it has not run or why its run leaves it out; its failure, every
    /// line of it; or what its agent said when it finished, as it wrote it.
    fn said(&self) -> Vec<Block> {
        self.stands().said()
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

    /// What the node says of itself as its own scene says it, where it ran
    /// (the word it stands at, what it took and spent, its tokens in and out
    /// kept after them), then what it is, in `look`'s glyphs.
    fn fields(&self, look: &Look) -> Fields {
        let node = &self.node;
        let runner = self
            .runner
            .as_ref()
            .map(|runner| runner_on(runner, &node.id, &self.state))
            .unwrap_or_default();
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
        self.stands()
            .printed_fields(look.glyphs)
            .push_if("kind", node.kind)
            .push_if(
                "attempt",
                node.attempt.map(|n| n.to_string()).unwrap_or_default(),
            )
            .push_if("runner", runner)
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
                let proof = proof.map(|proof| proof.blocks(look));
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
            Block::Heading(feed_heading(open_attempt_calls(&self.moments), look.glyphs)),
            fed.into(),
        ]
    }

    /// Where to read the rest of the run.
    fn next(&self) -> Next {
        command::next([SaidStep::of(&NextStep::status(&self.run_id))])
    }
}

#[cfg(test)]
mod tests;
