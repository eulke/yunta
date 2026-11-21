//! See [`super`]. A run parked on a child it runs: while the run leads, the child is the run
//! the stage shows, read whole and followed in its place, as a promotion's successor is; the
//! trail leads up to the run through the node that bore the child; and the ways back up.
//!
//! As a run that leads its screen comes to stand parked on a child (`ChildPaused`), the stage
//! moves onto the child: the child's pointers are its own from then on, the state follows it,
//! and the run is kept above it with the node that bore the child ([`Parent`]). A child parked on
//! a child of its own moves the stage on again, so the trail leads up through each. Which
//! child a run stands parked on is replay's to say (`WaitingOn::child_run`).
//!
//! The stage goes back up two ways. `esc`, with no prompt standing, goes up to the nearest
//! parent, whose node pointer becomes the person's on the node that bore the child, so the
//! parent does not lead and its own scenes stand; `f` there gives it the lead back, and the
//! child stands in its place again. And a parent the field shows standing no longer parked
//! on the run under it, its pause answered or moved on, takes the screen back, leading: the
//! field keeps every run at its head, the run shown's parents among them, while the run read
//! whole is the child. A parent the field does not hold is kept, as nothing says it moved.

use yunta_core::{NodeId, RunId};
use yunta_render::screen::Along;
use yunta_replay::frame::WaitingOn;

use super::lead::PointedBy;
use super::read_once::ReadOnce;
use super::{Framed, RunStage};
use crate::FollowedRuns;

/// A run whose child its stage shows in its place, and the node of it that bore the child:
/// what the trail leads up through, and what `esc` goes up to. Not `yunta-core`'s
/// `ChildLink`, the record a run's log keeps of a child it bore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Parent {
    run: RunId,
    node: NodeId,
}

impl RunStage {
    /// The stage moved onto the child the run `framed` stands parked on, where the run leads:
    /// the child shown in its place, and the run kept above it with the node that bore the
    /// child. Nothing where the run does not lead or stands parked on no child.
    pub(super) fn onto_its_child(&mut self, framed: &Framed<'_>) {
        let on = framed.frame.phase.waiting_on();
        let Some((node, child)) = on.and_then(WaitingOn::child_run).filter(|_| self.leads()) else {
            return;
        };
        let parent = Parent {
            run: self.run.clone(),
            node: node.clone(),
        };
        self.parents.push(parent);
        self.shows(child.clone());
    }

    /// Up to the nearest run whose child the stage shows, its node pointer the person's on
    /// the node that bore the child: whether there was one to go up to, where the stage shows
    /// a child.
    pub(super) fn up_to_its_parent(&mut self) -> bool {
        let Some(Parent { run, node }) = self.parents.pop() else {
            return false;
        };
        self.shows(run);
        self.node = PointedBy::ThePerson(node);
        true
    }

    /// Up to the outermost run whose child the stage shows that the field `followed` keeps
    /// standing no longer parked on the run under it, leading: it takes the screen back.
    /// Nothing where every parent the field holds still stands parked there.
    pub(super) fn up_where_a_parent_moved(&mut self, followed: &FollowedRuns) {
        let under = self.parents.iter().skip(1).map(|parent| &parent.run);
        let under: Vec<&RunId> = under.chain([&self.run]).collect();
        let moved = self.parents.iter().zip(under).position(|(parent, under)| {
            parked_on(followed, &parent.run).is_some_and(|child| child.as_ref() != Some(under))
        });
        let Some(at) = moved else {
            return;
        };
        let parent = self.parents.drain(at..).next();
        if let Some(parent) = parent {
            self.shows(parent.run);
        }
    }

    /// Which run bore the run the stage shows, and by which node, in render's words, where
    /// the stage shows a child in its parent's place: the nearest parent.
    pub(super) fn child_of(&self) -> Option<String> {
        let Parent { run, node } = self.parents.last()?;
        Some(yunta_render::children::child_of(run, node))
    }

    /// The places the trail names before the run shown: each run whose child the stage
    /// shows, by its handle, then the node of it that bore the child, outermost first.
    pub(super) fn trail_up(&self) -> impl Iterator<Item = String> + '_ {
        self.parents
            .iter()
            .flat_map(|parent| [parent.run.handle().to_string(), parent.node.to_string()])
    }

    /// The stage moved onto `run`, shown from its first line with nothing of the run shown
    /// before: its pointers the run's, its feed at its end, no pull request and no remark of
    /// its own said yet.
    fn shows(&mut self, run: RunId) {
        self.run = run;
        self.node = PointedBy::TheRun(None);
        self.finding = PointedBy::TheRun(None);
        self.along = Along::Findings;
        self.scroll.rewound();
        self.feed.back_at_its_end();
        self.remarked.clear();
        self.pull_request = ReadOnce::default();
    }
}

/// Where the field `followed` keeps shows `run` standing at its head: on the child it stands
/// parked on, or on none; nothing where the field does not hold it.
fn parked_on(followed: &FollowedRuns, run: &RunId) -> Option<Option<RunId>> {
    let held = followed.field.as_ref()?.runs.get(run)?;
    let head = held.state.clock().last_at()?;
    let phase = held.frame(head).phase;
    let child = phase.waiting_on().and_then(WaitingOn::child_run);
    Some(child.map(|(_, child)| child.clone()))
}
