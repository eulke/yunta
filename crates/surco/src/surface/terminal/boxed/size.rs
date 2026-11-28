//! See [`super`]. What a box needs over a screen before it is drawn, so an engine of cells
//! sizes it by Surco's own measure: as wide as its widest row, a cell in from the left upright,
//! two blank cells past it and the far upright, its title with the cells around it, up to the
//! most it is given; a row on the selection or a confirmation, every cell it is given; as tall
//! as all it holds between its edges.

use super::{insets, AROUND_TITLE, UPRIGHT};
use crate::components::Overlay;
use crate::line::Line;
use crate::surface::terminal::Terminal;
use crate::width::cell_width;

/// The blank cells a box as wide as what it holds keeps past its widest row, before its far
/// upright: two, as the design's help leaves them past its longest row.
const PAST_THE_WIDEST: usize = 2;

/// The cells and rows a box stands in over a screen. Not [`Paned`](crate::surface::Paned), what
/// it draws there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxSize {
    pub cells: usize,
    pub rows: usize,
}

impl Terminal {
    /// The cells and rows `overlay` boxed over a screen of this look's width stands in, given
    /// at most `most` cells: a confirmation, read as a sentence, every cell it is given; any
    /// other box its widest row inside its edges, with the blank cells past it and the far
    /// upright, a row on the selection reaching that upright, and its title with the cells
    /// around it in the top edge; as tall as all it holds. What an engine of cells sizes a box
    /// by before it draws one ([`Terminal::boxed`]).
    pub fn box_size(&self, overlay: &Overlay, most: usize) -> BoxSize {
        let rows = self.boxed(overlay, most, usize::MAX).lines.len();
        if overlay.confirms {
            return BoxSize { cells: most, rows };
        }
        let (inset, _) = insets(overlay);
        let held = self.held_in_a_box(overlay, most, usize::MAX);
        let needs = |line: &Line| match line.fill().is_some()
            || line.spans().iter().any(|span| span.fill.is_some())
        {
            true => most,
            false => {
                let opened = Line::new().plain(" ".repeat(inset)).then(line.clone());
                2 * UPRIGHT + cell_width(opened.text().trim_end()) + PAST_THE_WIDEST
            }
        };
        let widest = held.lines.iter().map(needs).max().unwrap_or(0);
        let titled = cell_width(&overlay.title) + AROUND_TITLE;
        BoxSize {
            cells: widest.max(titled).min(most),
            rows,
        }
    }
}

#[cfg(test)]
mod tests;
