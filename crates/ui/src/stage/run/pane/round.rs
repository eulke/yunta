//! See [`super`]. A round's pane, where a round of questions stands in place: with no title,
//! what the person asked the run for, where the run was asked for anything, then the round
//! under it (`in_place`). It takes the place of the run's nodes too, and no feed closes it:
//! the node's feed is on its own scene.
//!
//! Where the rows are too few, the blank row under the request goes first, then the request,
//! then the round gives way in its own order, as Surco lays a round out in a pane.

use surco::components::{Note, Region};
use surco::doc::{Block, Doc};
use surco::look::Look;

use super::super::{Framed, RunStage};

impl RunStage {
    /// The pane of the round the run `framed` waits on: what the person asked it for, where
    /// they asked for anything; `look_failed` said under it where a look failed; then the
    /// round.
    pub(super) fn round_pane(
        &self,
        framed: &Framed<'_>,
        look_failed: Option<&String>,
        look: &Look,
    ) -> Region {
        let request = framed.in_view.run.manifest.request();
        let asked = request.map(yunta_render::prompt::round::you_asked);
        let failed = look_failed.map(|failed| Block::from(Note(failed.clone())));
        let doc = Doc::new()
            .with_all(asked)
            .with_all(failed)
            .with_all(self.prompt_in_sight(framed, look));
        Region::new(doc)
    }
}
