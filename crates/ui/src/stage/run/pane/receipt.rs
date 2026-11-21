//! See [`super`]. A receipt's pane: what a run that finished was held to, each check in the
//! words its screen says them in, then what it spent, changed and opened, as render says
//! them; under a report, the findings that keep it from being done; and under all of it the
//! harvested field, which says nothing. It has no title: the trail names the receipt. No feed
//! closes it: a node's feed is on its own scene, one `j` or `k` away.
//!
//! Until the store verified the run's event chain, the pane holds nothing of the receipt;
//! where verifying failed, it says why in the receipt's place.

use surco::components::{Horizon, Note, Region};
use surco::doc::{Block, Doc};
use surco::line::Tone;
use surco::look::Look;
use yunta_render::blocks::FindingsThatBlock;
use yunta_render::page::{Receipt, Told};

use super::super::{Framed, RunStage};

impl RunStage {
    /// The receipt's pane of the run `framed`: its checks, its facts and the findings that
    /// block it, `look_failed` said under them where a look failed, over the harvested field;
    /// why its chain could not be verified in their place.
    pub(super) fn receipt_pane(
        &self,
        framed: &Framed<'_>,
        look_failed: Option<&String>,
        look: &Look,
    ) -> Region {
        let failed = look_failed.map(|failed| Block::from(Note(failed.clone())));
        let doc = match self.receipt_shown(framed) {
            Some(Ok(receipt)) => {
                let page = Receipt { receipt: &receipt };
                let told = Told::OnScreen(look.glyphs);
                let blocking = FindingsThatBlock::of(&framed.in_view.run.state);
                Doc::new()
                    .with(page.checks(told))
                    .with(page.facts(told))
                    .with_all(blocking.blocks(look.glyphs))
                    .with_all(failed)
                    .with(Horizon { tone: Tone::Done })
            }
            Some(Err(unverified)) => Doc::new().with(Note(unverified)).with_all(failed),
            None => Doc::new().with_all(failed),
        };
        Region::new(doc)
    }
}
