//! See [`crate`]. How a region taller than what a surface shows of it scrolls: the line a
//! person asked to see is kept here, and the keys move it from what a surface last laid
//! out, so every interface scrolls by the same keys and none keeps a position of its own.
//! A desk's pane and every stage that shows a page hold a [`Scroll`]; nothing else moves
//! what a region shows.

use surco::components::{Laid, Sight};

use crate::prompt::Stroke;

/// Where a person scrolled a region to, and what a surface last laid out of it, which the
/// keys that scroll move from. Not [`Sight`], what it asks a surface to show, nor
/// [`Laid`], what a surface showed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Scroll {
    /// The first of its lines the person asked to see.
    top: usize,
    /// What a surface last laid out of it.
    laid: Laid,
}

impl Scroll {
    /// What of the region the person asked to see.
    pub(crate) fn sight(&self) -> Sight {
        Sight::from_line(self.top)
    }

    /// The first of its lines the person asked to see.
    pub(crate) fn top(&self) -> usize {
        self.top
    }

    /// Whether more of the region stands than the last surface showed: where the keys
    /// that scroll have something to move.
    pub(crate) fn scrolls(&self) -> bool {
        self.laid.scrolls()
    }

    /// What a surface laid out of the region, which the next key moves from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        self.laid = laid;
    }

    /// Back at its first line, for a region that holds something else now.
    pub(crate) fn rewound(&mut self) {
        self.top = 0;
    }

    /// Whether `stroke` is one of the keys that scroll: what a stage whose page shares
    /// its keys with other work hands to [`Scroll::pressed`] first.
    pub(crate) fn moves_by(stroke: Stroke) -> bool {
        matches!(
            stroke,
            Stroke::Down
                | Stroke::Up
                | Stroke::PageDown
                | Stroke::PageUp
                | Stroke::Home
                | Stroke::End
                | Stroke::Insert(' ')
        )
    }

    /// Scrolls by the keys that move through what is read, never past what the last
    /// surface showed to be the end: a line with the arrows, a page with PageUp,
    /// PageDown or the space bar, the first line with Home and the last with End. A page
    /// moves by what was in sight, which leaves out the row that counts what is below, so
    /// a person who pages through it reads every line. A key that scrolls nothing leaves
    /// it where it is.
    pub(crate) fn pressed(&mut self, stroke: Stroke) {
        let Laid { top, page, most } = self.laid;
        let in_sight = page.saturating_sub(1).max(1);
        self.top = match stroke {
            Stroke::Down => (top + 1).min(most),
            Stroke::Up => top.saturating_sub(1),
            Stroke::PageDown | Stroke::Insert(' ') => (top + in_sight).min(most),
            Stroke::PageUp => top.saturating_sub(in_sight),
            Stroke::Home => 0,
            Stroke::End => most,
            _ => return,
        };
    }
}
