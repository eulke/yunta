//! See [`super`]. The feed at the foot of a run's pane: what the node pointed at does, a row
//! a moment, in render's words.
//!
//! The pane ends with the feed of the node pointed at in a scene that ends with one (the
//! work, the board, the findings and a node's own), each of which composes its pane,
//! whoever points at the node. A feed that lists nothing yet draws no row, and its place is
//! still the pane's, so a node's first moment comes into sight at the feed's end. Render is
//! handed only the rows around where the feed stands, its window (`Scroll::window`), with
//! how many stand above and under them, from the moments of the node its feed lists, which
//! the run in view keeps listed as they come, with its open attempt's calls counted: a
//! drawing reads the moments in sight and no other, however many calls the node made. The
//! window is counted from the feed's first row, as Surco lays a feed out, so a place stays on
//! the rows in sight whatever grows under them.
//!
//! The feed opens at its end and stays there as it grows, the run's while the run leads; a
//! person who scrolls it back takes it, and the run's news move it no more, nor either
//! pointer (`lead`). `End`, scrolling to its last row or `f` give it back. It goes back to
//! its end as it becomes another node's feed: the person moving along the nodes, a successor
//! in its run's place. Only the feed scrolls in a pane that ends with one: the keys that
//! scroll and what a surface laid out are the feed's there and the pane's own anywhere else,
//! so scrolling a pane takes nothing from the run.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use surco::components::Feed;
use surco::look::Look;
use yunta_core::{NodeId, SessionId, TaskId};
use yunta_render::chronicle;
use yunta_replay::frame::Moment;

use super::{Framed, RunStage};
use crate::scroll::Scroll;
use crate::FollowedRuns;

impl RunStage {
    /// The node whose feed the pane ends with on the run `framed`: the one pointed at, in a
    /// scene that ends with a node's feed. None anywhere else.
    pub(super) fn fed(&self, framed: &Framed<'_>) -> Option<NodeId> {
        let scene = self.scene_of(framed)?;
        let ends_with_it = scene.ends_with_its_feed();
        ends_with_it.then(|| self.pointed_in(framed)).flatten()
    }

    /// Where the pane of the run `followed` keeps stands at `now`, and what a surface last
    /// laid out of it: its feed's place where it ends with one, its own anywhere else.
    pub(crate) fn pane(&self, followed: &FollowedRuns, now: DateTime<Utc>) -> Scroll {
        *self.pane_in(self.framed(followed, now).as_ref())
    }

    /// Where the pane of the run `framed` stands: what is read in the prompt in place where
    /// one stands, what stands above a card or the pane of the plan's desk, its feed's place
    /// where it ends with one, its own anywhere else.
    pub(super) fn pane_in(&self, framed: Option<&Framed<'_>>) -> &Scroll {
        if let Some(held) = self.in_place_in(framed) {
            return held.scrolled();
        }
        match framed.and_then(|framed| self.fed(framed)) {
            Some(_) => &self.feed,
            None => &self.scroll,
        }
    }

    /// Where the pane of the run `framed` stands, for a key that scrolls it or what a
    /// surface laid out of it.
    pub(super) fn pane_in_mut(&mut self, framed: Option<&Framed<'_>>) -> &mut Scroll {
        let in_place = self.in_place_in(framed).is_some();
        let fed = framed.and_then(|framed| self.fed(framed)).is_some();
        match (in_place, fed, self.in_place.as_deref_mut()) {
            (true, _, Some(held)) => held.scrolled_mut(),
            (true, true, None) | (false, true, _) => &mut self.feed,
            (true, false, None) | (false, false, _) => &mut self.scroll,
        }
    }

    /// Where among the moments of the run `framed` the moments of `node` its feed lists
    /// stand, of every attempt, in the log's order, as the run in view keeps them.
    pub(super) fn listed<'a>(&self, framed: &Framed<'a>, node: &NodeId) -> &'a [usize] {
        framed.in_view.fed(node)
    }

    /// The feed of `node` of the run `framed`, which lists the moments at `listed`, in its
    /// window where the feed stands, in render's words.
    pub(super) fn feed_of(
        &self,
        framed: &Framed<'_>,
        node: &NodeId,
        listed: &[usize],
        look: &Look,
    ) -> Feed {
        let none = BTreeMap::new();
        let tasks = framed
            .in_view
            .run
            .state()
            .nodes
            .get(node)
            .map_or(&none, |record| &record.session_tasks);
        window(listed, &framed.in_view.moments, tasks, &self.feed, look)
    }
}

/// The feed of a node whose feed lists the moments of `moments` at `listed`, of every attempt
/// in the log's order, in its window where `scroll` stands (`Scroll::window`), its calls
/// naming the task of their session by `tasks`, how many stand above and under the rows it
/// holds counted. Only the moments in the window are read.
pub(super) fn window(
    listed: &[usize],
    moments: &[Moment],
    tasks: &BTreeMap<SessionId, TaskId>,
    scroll: &Scroll,
    look: &Look,
) -> Feed {
    let rows = listed.len();
    let held = scroll.window(rows);
    let (from, to) = (held.start, held.end);
    let in_sight = listed.get(held).unwrap_or_default();
    let in_sight: Vec<Moment> = in_sight
        .iter()
        .filter_map(|at| moments.get(*at))
        .cloned()
        .collect();
    chronicle::feed(&in_sight, tasks, from, rows - to, look)
}

#[cfg(test)]
mod tests;
