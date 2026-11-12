//! A region of a screen drawn in a pane: the one rule an engine of cells draws a region
//! by, so every surface and every test of one draws a region the same way. A region whose
//! document holds a prompt holds it, fitted to the rows the blocks around it leave, with
//! the cursor where it is typed into; any other region scrolls from what of it the person
//! asked to see. A pane is not a terminal's bottom row, so a prompt in one fills every row
//! it is given, where one held where a person is ([`Terminal::held`]) keeps the last.

use super::Terminal;
use crate::components::{Laid, Region, Sight};
use crate::doc::Doc;
use crate::line::Line;

/// What a pane shows of a region: its lines, where the cursor stands when a prompt in it
/// is typed into (the row, counted from the pane's first, and the cell), and what was
/// laid out of what scrolls in it. Not [`Placed`](super::Placed): what a prompt held where
/// a person answers it draws.
pub struct Paned {
    pub lines: Vec<Line>,
    pub cursor: Option<(usize, usize)>,
    pub laid: Laid,
}

impl Terminal {
    /// `region` drawn in a pane `rows` high, at this look's width, which is the pane's. A
    /// region whose document holds a prompt draws the blocks under the prompt whole, the
    /// blocks above it in at most half of what those leave, giving way from their top, and
    /// holds the prompt in the rest; a prompt that cannot be asked in the rest takes the
    /// rows it needs from the blocks under it, which give way from their end. Any other
    /// region scrolls from its sight.
    pub fn pane(&self, region: &Region, rows: usize) -> Paned {
        self.paned(&region.doc, region.sight, rows)
    }

    /// `doc`, of which `sight` is what a person asked to see, drawn in a pane `rows` high
    /// by the rule [`Terminal::pane`] states: what a region and what is open over a screen
    /// are drawn by.
    pub(super) fn paned(&self, doc: &Doc, sight: Sight, rows: usize) -> Paned {
        let Some(at) = doc.prompt_at() else {
            let (lines, laid) = self.scrolled(doc, sight, rows);
            return Paned {
                lines,
                cursor: None,
                laid,
            };
        };
        let blocks = doc.blocks();
        let below = parted(self.in_rhythm(&blocks[at + 1..]), Side::Below);
        let left = rows.saturating_sub(below.len());
        let mut above = parted(self.in_rhythm(&blocks[..at]), Side::Above);
        // What stands above a prompt is what it is about: it keeps the lines nearest the
        // prompt, and never more than half, so what a person answers stays in sight.
        let kept_above = above.len().min(left / 2);
        above.drain(..above.len() - kept_above);
        let held = self.held_in(&blocks[at], self.look.width.cells(), left - kept_above);
        let cursor = held
            .cursor
            .map(|(row, cell)| (row + kept_above, cell))
            .filter(|(row, _)| *row < rows);
        // A prompt given fewer rows than it can be asked in still draws those: what stands
        // under it gives way from its end, since what a person answers, and the row they
        // type on, stay in sight.
        let mut lines = above;
        lines.extend(held.lines);
        lines.extend(below);
        lines.truncate(rows);
        Paned {
            lines,
            cursor,
            laid: held.laid.unwrap_or_default(),
        }
    }
}

/// Which side of a prompt blocks stand on.
enum Side {
    Above,
    Below,
}

/// `lines` with the blank row that parts them from a prompt on `side`; nothing for none.
fn parted(mut lines: Vec<Line>, side: Side) -> Vec<Line> {
    if lines.is_empty() {
        return lines;
    }
    match side {
        Side::Above => lines.push(Line::new()),
        Side::Below => lines.insert(0, Line::new()),
    }
    lines
}

#[cfg(test)]
mod tests;
