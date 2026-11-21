//! See [`super`]. The plan's desk in place: the plan the gate shows, read a place at a time,
//! the gate's options at its foot (`in_place`). It heads itself and takes the whole pane, with
//! no title of the node's: its head names the plan, the run and the node it waits at. No feed
//! closes it: the node's feed is on its own scene.

use surco::components::{Note, Region};
use surco::doc::{Block, Doc};
use surco::look::Look;

use super::super::{Framed, RunStage};

impl RunStage {
    /// The pane of the plan's desk the run `framed` waits on: the desk, then `look_failed`
    /// said under it where a look failed.
    pub(super) fn desk_pane(
        &self,
        framed: &Framed<'_>,
        look_failed: Option<&String>,
        look: &Look,
    ) -> Region {
        let failed = look_failed.map(|failed| Block::from(Note(failed.clone())));
        let doc = Doc::new()
            .with_all(self.prompt_in_sight(framed, look))
            .with_all(failed);
        Region::new(doc)
    }
}
