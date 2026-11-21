//! See [`crate`]. How a region taller than what a surface shows of it scrolls: the place a
//! person asked to see is kept here, and the keys move it from what a surface last laid
//! out, so every interface scrolls by the same keys and none keeps a position of its own.
//! A desk's pane and every stage that shows a page hold a [`Scroll`]; nothing else moves
//! what a region shows.
//!
//! A region stands at one of two places ([`ScrolledTo`]): at its end, where what grows
//! under it comes into sight as it grows, or at a row, which stays where it is whatever
//! grows under it. A key that reaches the last row a surface laid out goes to the end, so
//! a region a person pages to the bottom of keeps its newest lines in sight; any other
//! key that scrolls goes to a row. A page opens at its first row; a feed opens at its end
//! ([`Scroll::at_the_end`]), and goes back there when what it follows is given it again.
//!
//! Where a region stands and whether the last surface left it anything to scroll are read
//! from outside ([`Scroll::to`], [`Scroll::scrolls`]): no row of a screen says the second,
//! so a reader that tells states apart by what they show reads it here.
//!
//! A feed hands a drawing only the rows around where it stands ([`Scroll::window`]): those
//! the last surface laid out of it and [`MARGIN`] more on each side, counted from its first
//! row, so a feed of any length costs a drawing its window.

use std::ops::Range;

use surco::components::{Laid, Sight};

use crate::prompt::Stroke;

/// The rows of a feed a drawing is handed beyond those the last surface laid out of it, on
/// each side of them: what a pane shows before its next layout where it grew taller, a
/// terminal made taller or the blocks above the feed giving way, and the whole of a first
/// drawing, which no layout precedes, on a terminal of some seventy rows.
pub(crate) const MARGIN: usize = 64;

/// Where a person scrolled a region to: its end, or the row it shows first. Not
/// [`Sight`], what a surface is asked to show of it, which this maps onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrolledTo {
    /// Its last rows, however many it grows to.
    End,
    /// From this row on, counted from its first.
    Row(usize),
}

/// Where a person scrolled a region to, and what a surface last laid out of it, which the
/// keys that scroll move from. Not [`Sight`], what it asks a surface to show, nor
/// [`Laid`], what a surface showed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scroll {
    /// Where the person scrolled it to.
    to: ScrolledTo,
    /// What a surface last laid out of it.
    laid: Laid,
}

impl Default for Scroll {
    /// A region read from its first row, as a page opens.
    fn default() -> Self {
        Scroll {
            to: ScrolledTo::Row(0),
            laid: Laid::default(),
        }
    }
}

impl Scroll {
    /// A region read from its end, as a feed opens: what grows under it comes into sight
    /// as it grows, until a person scrolls it back.
    pub(crate) fn at_the_end() -> Self {
        Scroll {
            to: ScrolledTo::End,
            laid: Laid::default(),
        }
    }

    /// Where the person scrolled it to.
    pub fn to(&self) -> ScrolledTo {
        self.to
    }

    /// What of the region the person asked to see.
    pub(crate) fn sight(&self) -> Sight {
        match self.to {
            ScrolledTo::End => Sight::at_the_end(),
            ScrolledTo::Row(row) => Sight::from_line(row),
        }
    }

    /// Whether more of the region stands than the last surface showed: where the keys
    /// that scroll have something to move.
    pub fn scrolls(&self) -> bool {
        self.laid.scrolls()
    }

    /// What a surface last laid out of the region: none of it before the first surface
    /// draws it.
    pub(crate) fn laid_last(&self) -> Laid {
        self.laid
    }

    /// What a surface laid out of the region, which the next key moves from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        self.laid = laid;
    }

    /// The rows of a feed of `rows` rows a drawing is handed where it stands: at its end, the
    /// newest rows a page of the last layout shows and [`MARGIN`] more above them; at a row, a
    /// page from it and [`MARGIN`] more on each side. Those before and after them stand
    /// counted.
    pub(crate) fn window(&self, rows: usize) -> Range<usize> {
        let page = self.laid_last().page;
        match self.to {
            ScrolledTo::End => rows.saturating_sub(page + MARGIN)..rows,
            ScrolledTo::Row(row) => {
                let from = row.saturating_sub(MARGIN).min(rows);
                let to = row.saturating_add(page + MARGIN).min(rows);
                from..to.max(from)
            }
        }
    }

    /// Back at its first line, for a region that holds something else now.
    pub(crate) fn rewound(&mut self) {
        self.to = ScrolledTo::Row(0);
    }

    /// Back at its end, given again to what grows under it: what a feed scrolled back
    /// comes to when the person gives it back, or when it holds another feed now.
    pub(crate) fn back_at_its_end(&mut self) {
        self.to = ScrolledTo::End;
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
    /// PageDown or the space bar, the first line with Home and the end with End. A page
    /// moves by what was in sight, which leaves out the row that counts what is below, so
    /// a person who pages through it reads every line. A key that reaches the last first
    /// line the surface laid out goes to the end, any other to a row. A key that scrolls
    /// nothing, or a region the last surface showed whole, leaves it where it is.
    pub(crate) fn pressed(&mut self, stroke: Stroke) {
        if !self.scrolls() {
            return;
        }
        let Laid { top, page, most } = self.laid;
        let in_sight = page.saturating_sub(1).max(1);
        let row = match stroke {
            Stroke::Down => top + 1,
            Stroke::Up => top.saturating_sub(1),
            Stroke::PageDown | Stroke::Insert(' ') => top + in_sight,
            Stroke::PageUp => top.saturating_sub(in_sight),
            Stroke::Home => 0,
            Stroke::End => most,
            _ => return,
        };
        self.to = match row >= most {
            true => ScrolledTo::End,
            false => ScrolledTo::Row(row),
        };
    }
}
