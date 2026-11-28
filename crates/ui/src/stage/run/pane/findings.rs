//! See [`super`]. The findings' pane: the run's findings at the one the finding pointer
//! stands on, then the feed of the node that reviews or fixes them. It has no node's title:
//! the findings' heading, how many were answered of how many were posted and which nodes
//! posted them, takes its title's row, cut at the row's edge as one.
//!
//! Short of rows, what stands above the feed says itself in fewer rows: the finding pointed
//! at gives way first, the list keeping every row and its answers; then the list gives way
//! whole, the heading, whose count stands for it, standing at the least terminal.

use surco::components::{Note, Region};
use surco::doc::{Block, Doc};
use surco::look::Look;
use yunta_core::NodeId;
use yunta_render::blocks::FindingsAt;

use super::super::{Framed, RunStage};

impl RunStage {
    /// The findings' pane of the run `framed`, over the feed of the node `pointed`: titled by
    /// the findings' heading, the run's findings at the one pointed at, `look_failed` said
    /// under them where a look failed, then the feed under its heading; said again in fewer
    /// rows, in the order the findings let their parts go.
    pub(super) fn findings_pane(
        &self,
        framed: &Framed<'_>,
        pointed: &NodeId,
        look_failed: Option<&String>,
        look: &Look,
    ) -> Region {
        let view = framed.in_view.run.state().findings.standing();
        let at = self.finding_pointed_in(framed);
        let at = at.as_ref().map(|(node, id)| (node.as_ref(), id));
        let findings = FindingsAt::of(&view, at);
        let glyphs = look.glyphs;
        let failed = || look_failed.map(|failed| Block::from(Note(failed.clone())));
        let abridged = vec![
            std::iter::once(findings.list()).chain(failed()).collect(),
            failed().into_iter().collect(),
        ];
        let doc = Doc::new()
            .with_all(findings.blocks(glyphs))
            .with_all(failed())
            .with_all(self.feed_under_its_heading(framed, pointed, look));
        Region::new(doc)
            .titled([findings.heading(glyphs)])
            .abridged(abridged)
    }
}
