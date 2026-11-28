//! See [`super`]. A standstill's pane: under the title of the node pointed at, why the run
//! stands with nothing to answer here, as render says it: what it waits on and what moves it
//! again, or where it stopped and why. No feed closes it: the node's feed is on its own
//! scene, one `j` or `k` away.
//!
//! Short of rows, what the run waits on gives way whole: what stopped it and what moves it
//! again stay.

use surco::components::{Note, Region};
use surco::doc::{Block, Doc};
use surco::look::Look;
use yunta_core::NodeId;
use yunta_render::blocks::WhereItStopped;

use super::super::{Framed, RunStage};
use super::titled;

impl RunStage {
    /// The pane of the run `framed`, standing still, for the node `pointed`: its title, why
    /// the run stands, `look_failed` said under it where a look failed; said again in fewer
    /// rows, what the run waits on given way.
    pub(super) fn standstill_pane(
        &self,
        framed: &Framed<'_>,
        pointed: &NodeId,
        look_failed: Option<&String>,
        look: &Look,
    ) -> Region {
        let manifest = framed.in_view.run.manifest();
        let stopped = WhereItStopped::of(framed.frame, &framed.times, manifest);
        let failed = || look_failed.map(|failed| Block::from(Note(failed.clone())));
        let least = stopped
            .iter()
            .flat_map(|stopped| stopped.least(look.glyphs));
        let abridged = vec![least.chain(failed()).collect()];
        let doc = Doc::new()
            .with_all(
                stopped
                    .iter()
                    .flat_map(|stopped| stopped.blocks(look.glyphs)),
            )
            .with_all(failed());
        titled(Region::new(doc).abridged(abridged), framed, pointed)
    }
}
