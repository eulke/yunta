//! See [`super`]. A region whose document ends with a horizon: the field in perspective under
//! what a screen says, which says nothing itself.
//!
//! Its blocks are laid out first, as a region with no horizon lays them out, a blank row
//! between two; a line asked among them is held as a prompt is, with what stands above it in
//! at most half of the rows what stands under it leaves, and the cursor where it is typed
//! into. The horizon takes the rows they leave, a blank row parting it from them, drawn in
//! every one of them. Given fewer than the least it stands in, no braille to draw it in, or a
//! pane that stands close, which keeps no blank row, it gives way whole: it says nothing a
//! person loses, so the blocks stand where they stood and the rows it would take stay blank.
//! Short of rows for the blocks themselves, their blank rows give way before their rows do,
//! then what stands under the window is counted, as a pane read from its top counts it.
//!
//! Words set to a measure stand in their column (`measured`), and the horizon spans the pane
//! under them, since it is no row of words. A screen that opens on a few words over the field
//! sets them so, lowered, and on a pane of [`LOWERED_FROM`] rows and more they stand
//! [`LOWERED_BY`] rows lower, the horizon taking what they leave; words set to a measure that
//! is not lowered, as a setup's rows, stand where they fall.

use super::{closed_up, Paned, Spacing, Terminal};
use crate::components::{Horizon, Measure, Sight};
use crate::doc::Block;
use crate::line::Line;

/// The fewest rows a pane holds for words set to a measure over the field to stand lower in
/// it: more than 26, as the design lowers the first request's name on its wide screen, whose
/// regions hold 32 rows, and not on its narrow one, whose regions hold 20 and keep them for
/// the field.
const LOWERED_FROM: usize = 27;

/// The rows words set to a measure over the field stand lower on a tall pane: two, so the name
/// in block letters they open with stands off the rule, as the design draws it.
const LOWERED_BY: usize = 2;

impl Terminal {
    /// `above`, the blocks of a region over the horizon `horizon` its document ends with,
    /// in a pane `rows` high spaced by `spacing`, set to `measure` where it gives one, the
    /// first of `abridged` that fits standing for them where they do not, scrolled from
    /// `sight` where they still do not: the blocks, then the horizon in every row they leave
    /// after a blank row, where it stands.
    pub(super) fn over_a_horizon(
        &self,
        (above, horizon): (&[Block], &Horizon),
        abridged: &[Vec<Block>],
        sight: Sight,
        rows: usize,
        spacing: Spacing,
        measure: Option<Measure>,
    ) -> Paned {
        let lowers = measure.is_some_and(|measure| measure.lowered);
        let lowered = match lowers && rows >= LOWERED_FROM {
            true => LOWERED_BY,
            false => 0,
        };
        let room = rows - lowered;
        let words = match self.column(measure) {
            Some(column) => {
                let drawn = column
                    .words
                    .above_the_field(above, abridged, sight, room, spacing);
                column.placed(drawn)
            }
            None => self.above_the_field(above, abridged, sight, room, spacing),
        };
        let mut lines: Vec<Line> = std::iter::repeat_with(Line::new).take(lowered).collect();
        let parted = usize::from(!words.lines.is_empty());
        let left = room.saturating_sub(words.lines.len() + parted);
        lines.extend(words.lines);
        let field = match spacing {
            Spacing::Aired => self.horizon(horizon, left),
            Spacing::Close => Vec::new(),
        };
        if !field.is_empty() {
            lines.extend(std::iter::repeat_with(Line::new).take(parted));
            lines.extend(field);
        }
        Paned {
            lines,
            cursor: words.cursor.map(|(row, cell)| (row + lowered, cell)),
            laid: words.laid,
        }
    }

    /// `above`, the blocks over a horizon, in a pane `rows` high spaced by `spacing`: a line
    /// asked among them held as a prompt is, the cursor where it is typed into; any other
    /// blocks whole where they fit, else the first of `abridged` that fits, scrolled from
    /// `sight` where none does.
    fn above_the_field(
        &self,
        above: &[Block],
        abridged: &[Vec<Block>],
        sight: Sight,
        rows: usize,
        spacing: Spacing,
    ) -> Paned {
        if let Some(at) = above.iter().position(|block| block.prompt().is_some()) {
            return self.prompted(above, at, rows, spacing);
        }
        let fitted = self.fitted(above, abridged, rows, spacing);
        let lines = match fitted.len() > rows {
            true => closed_up(fitted, Spacing::Close),
            false => fitted,
        };
        let (lines, laid) = self.scrolled_lines(&lines, sight, rows);
        Paned {
            lines,
            cursor: None,
            laid,
        }
    }
}

#[cfg(test)]
mod tests;
