//! See [`super`]. The pane of a run whose process asks a question at its own terminal: under
//! the title of the node it asks about, the question as render says it, the process that asks
//! and since when, and that nothing here answers it. It only reads: no key on its scene
//! answers the question, which stands on no log until it is answered there. No feed closes
//! it: the node's feed is on its own scene, one `j` or `k` away.

use surco::components::{Note, Region};
use surco::doc::{Block, Doc};
use yunta_core::NodeId;
use yunta_render::blocks::AskedAtItsTerminal;

use super::super::{Framed, RunStage};
use super::titled;

impl RunStage {
    /// The pane of the run `framed`, its process asking at its own terminal, for the node
    /// `pointed`: its title, the question, who asks it and since when, as the registry its
    /// frame read says them, then that nothing here answers it; `look_failed` said under it
    /// where a look failed.
    pub(super) fn asked_elsewhere_pane(
        &self,
        framed: &Framed<'_>,
        pointed: &NodeId,
        look_failed: Option<&String>,
    ) -> Region {
        let asked = framed
            .frame
            .asking
            .as_ref()
            .map(|asking| AskedAtItsTerminal { asking }.blocks());
        let failed = look_failed.map(|failed| Block::from(Note(failed.clone())));
        let doc = Doc::new()
            .with_all(asked.into_iter().flatten())
            .with_all(failed);
        titled(Region::new(doc), framed, pointed)
    }
}
