//! See [`super`]. A loop's pane, its board: under its title, each task at work, in the order
//! the log registered them, its own row with its title from the tasks document handed over
//! and its attempt, turning while the run moves; one step under it how it is proved and its
//! scope. Then the loop's one feed, under every task and one step under them, each call
//! naming its task where several work at once.
//!
//! A task at work is one the loop is on: running, or blocked on what it cannot pass on its
//! own. One done, failed, or not begun stands in the nodes beside the pane alone.
//!
//! Short of rows, what stands above the feed says itself in fewer rows, in the order a board
//! lets it go: each scope's paths with their heading, then the blank row under each task's
//! own row, then everything but the tasks' own rows, which say what the loop works on and
//! stand at the least terminal. What holds the loop still, where its work stands still,
//! stands above them all at every size.

use std::collections::BTreeMap;

use surco::components::{Note, Region, Section};
use surco::doc::{Block, Doc};
use surco::line::{Line, Tone};
use surco::look::Look;
use yunta_api::Document;
use yunta_core::events::{ArtifactId, TaskStatus};
use yunta_core::shown::ShownContent;
use yunta_core::{ArtifactKind, NodeId, TaskId};
use yunta_render::blocks::{TaskProof, TaskScope};
use yunta_render::chronicle;

use super::super::{Framed, RunStage};
use super::{in_the_cards_place, still, titled};
use crate::RunInView;

impl RunStage {
    /// The board of the loop `pointed` of the run `framed`: its title, each task at work with
    /// its proof and its scope, `look_failed` said under them where a look failed, then the
    /// loop's feed one step under them, under its heading where it lists anything; said
    /// again in fewer rows, in the order a board lets its parts go.
    pub(super) fn board_pane(
        &self,
        framed: &Framed<'_>,
        pointed: &NodeId,
        look_failed: Option<&String>,
        look: &Look,
    ) -> Region {
        let tasks = tasks_at_work(framed);
        let still = || still(framed, look);
        let failed = || look_failed.map(|failed| Block::from(Note(failed.clone())));
        let abridged = abridged(&tasks, look)
            .into_iter()
            .map(|above| still().into_iter().chain(above).chain(failed()).collect())
            .collect();
        let doc = Doc::new()
            .with_all(still())
            .with_all(whole(&tasks, look))
            .with_all(failed())
            .with(self.feed_one_step_under(framed, pointed, look));
        titled(Region::new(doc).abridged(abridged), framed, pointed)
    }

    /// The feed of the loop `pointed` of the run `framed`, one step under its tasks, under its
    /// heading, which counts the open attempt's calls, where it lists anything; where what
    /// the run waits on a person for could not be read, the card standing in its place.
    fn feed_one_step_under(&self, framed: &Framed<'_>, pointed: &NodeId, look: &Look) -> Block {
        if let Some(why) = self.unread_in_its_pane(framed) {
            return in_the_cards_place(framed, why);
        }
        let listed = self.listed(framed, pointed);
        let feed = self.feed_of(framed, pointed, &listed, look);
        if listed.is_empty() {
            return feed.into();
        }
        let calls = chronicle::open_attempt_calls(&listed);
        let heading = chronicle::feed_heading(calls, look.glyphs);
        Block::Section(Section {
            lead: None,
            title: Line::new().push(Tone::Muted, heading),
            blocks: vec![feed.into()],
        })
    }
}

/// Each task at work of the run `framed`, in the order the log registered them, its proof
/// titled by the tasks document handed over and turning while the run moves, with its scope.
fn tasks_at_work(framed: &Framed<'_>) -> Vec<(TaskProof, Option<TaskScope>)> {
    let state = &framed.in_view.run.state;
    let titles = titles(framed.in_view);
    let turned = state.head().map(|head| head.get());
    let turned = turned.filter(|_| framed.frame.moves());
    let mut registered: Vec<_> = state.tasks.iter().collect();
    registered.sort_by_key(|(id, task)| (task.registered_at, *id));
    registered
        .into_iter()
        .filter(|(_, task)| at_work(task.status))
        .filter_map(|(id, _)| {
            let title = titles.get(id).map(String::as_str);
            let proof = TaskProof::of(id, title, state)?;
            let proof = match turned {
                Some(turned) => proof.turning(turned),
                None => proof,
            };
            Some((proof, TaskScope::of(id, state)))
        })
        .collect()
}

/// What `tasks` say above the feed, whole: each task's row and how it is proved, apart, then
/// its scope.
fn whole(tasks: &[(TaskProof, Option<TaskScope>)], look: &Look) -> Vec<Block> {
    tasks
        .iter()
        .flat_map(|(proof, scope)| {
            let scope = scope.iter().flat_map(|scope| scope.blocks(look.glyphs));
            proof.blocks(look).into_iter().chain(scope)
        })
        .collect()
}

/// What `tasks` say above the feed in fewer rows, each in fewer than the one before: their
/// scopes left out; then each proof right under its task's row; then the tasks' own rows
/// alone. None where no task is at work.
fn abridged(tasks: &[(TaskProof, Option<TaskScope>)], look: &Look) -> Vec<Vec<Block>> {
    if tasks.is_empty() {
        return Vec::new();
    }
    let unscoped = tasks.iter().flat_map(|(proof, _)| proof.blocks(look));
    let close = tasks.iter().map(|(proof, _)| {
        let row = std::iter::once(proof.row(look));
        Block::Rows(row.chain(proof.proof()).collect())
    });
    let rows = tasks.iter().map(|(proof, _)| proof.row(look)).collect();
    vec![unscoped.collect(), close.collect(), vec![Block::Rows(rows)]]
}

/// Whether a task in `status` is one its loop is on: running, or blocked on what it cannot
/// pass on its own.
fn at_work(status: TaskStatus) -> bool {
    match status {
        TaskStatus::Running | TaskStatus::Blocked => true,
        TaskStatus::Pending | TaskStatus::Ready | TaskStatus::Done | TaskStatus::Failed => false,
    }
}

/// The title the tasks document of the run `in_view` gives each task it lists: the latest one
/// the run holds, as its node handed it over and the follower read it. None where it is not
/// read.
fn titles(in_view: &RunInView) -> BTreeMap<TaskId, String> {
    let plan = ArtifactId::Interpreted {
        kind: ArtifactKind::Tasks,
    };
    let latest = in_view.run.state.artifacts.latest(&plan, None);
    let producer = latest.and_then(|held| held.producer.as_ref());
    let handed = producer.and_then(|producer| in_view.handed_over.get(producer));
    let review = handed
        .into_iter()
        .flatten()
        .find_map(|document| match document {
            Document::Read(read) => match &read.content {
                ShownContent::Tasks(review) => Some(review),
                _ => None,
            },
            Document::Unread { .. } => None,
        });
    review
        .into_iter()
        .flat_map(|review| &review.plan.tasks)
        .map(|task| (task.id.clone(), task.title.clone()))
        .collect()
}
