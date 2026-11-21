//! See [`super`]. A node's own pane, for the node the person points at while the run does
//! not lead and the node is not at work: under its title, how it stands, as render says it
//! for its own page; then its feed whole, every attempt's moments, each attempt named where
//! it opens, under its heading where it lists anything.
//!
//! How it stands, by what the node came to: what the person's decision holds it on, where
//! the run waits on them at it; that it has not run yet; why the run's mode, or the run,
//! leaves it out; else the word it stands at, what it took and spent, and what it came to.
//!
//! Short of rows, what stands above the feed says itself in fewer rows: the word it stands
//! at alone, or the sentence that it did not run; what a decision holds it on gives way
//! whole, the attention row saying it.

use surco::components::{Note, Region};
use surco::doc::{Block, Doc};
use surco::look::Look;
use yunta_core::NodeId;
use yunta_render::blocks::HowItStands;

use super::super::{Framed, RunStage};
use super::titled;

impl RunStage {
    /// The pane of the node `pointed` of the run `framed` as its own scene shows it: its
    /// title, how it stands, `look_failed` said under it where a look failed, then its feed
    /// whole, under its heading where it lists anything; said again in fewer rows, how it
    /// stands at the least.
    pub(super) fn node_pane(
        &self,
        framed: &Framed<'_>,
        pointed: &NodeId,
        look_failed: Option<&String>,
        look: &Look,
    ) -> Region {
        let frame = &framed.frame;
        let waited_at = frame.waits_on_a_person_at() == Some(pointed);
        let node = frame.nodes.iter().find(|node| node.id == *pointed);
        let stands = node.map(|node| HowItStands::of(node, &frame.mode, waited_at));
        let failed = || look_failed.map(|failed| Block::from(Note(failed.clone())));
        let least = stands.iter().flat_map(HowItStands::least);
        let abridged = vec![least.chain(failed()).collect()];
        let doc = Doc::new()
            .with_all(stands.iter().flat_map(HowItStands::blocks))
            .with_all(failed())
            .with_all(self.feed_under_its_heading(framed, pointed, look));
        titled(Region::new(doc).abridged(abridged), framed, pointed)
    }
}
