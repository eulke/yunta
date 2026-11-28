//! See [`super`]. A line typed or a list picked from on a page of its own, what asks it the
//! only block above it and nothing under it: the new workflow's name, a pack's source, the
//! documents whose shape a person reads. A line asked alone in an aired pane opens on a blank
//! row, as a form opens, where the pane holds the line and that row, the blank row giving way
//! before any row of the line; on a screen standing short of rows, either stands close, what
//! asks it right over it and a line asked alone on no blank row.

use super::{Paned, Terminal};
use crate::doc::Block;
use crate::line::Line;

impl Terminal {
    /// `line`, a line asked alone in an aired pane `rows` high: on a blank row first, as a form
    /// opens, where the pane holds the line and that row; the blank row gives way before any
    /// row of the line does.
    pub(super) fn line_asked_alone(&self, line: &Block, rows: usize) -> Paned {
        let held = self.held_in(line, self.look.width.cells(), rows);
        let opens = usize::from(held.lines.len() < rows);
        let mut lines = vec![Line::new(); opens];
        lines.extend(held.lines);
        lines.truncate(rows);
        let cursor = held.cursor.map(|(row, cell)| (row + opens, cell));
        Paned {
            lines,
            cursor: cursor.filter(|(row, _)| *row < rows),
            laid: held.laid.unwrap_or_default(),
        }
    }
}

/// Whether `blocks` ask a line typed or a pick from a list on a page of their own: the prompt
/// they hold is one of those, the last of them. On a screen standing short of rows they
/// stand close, what asks standing right over it.
pub(super) fn asked_on_a_page(blocks: &[Block]) -> bool {
    matches!(
        blocks.last(),
        Some(Block::TypedAnswer(_) | Block::ChoiceList(_))
    ) && blocks[..blocks.len() - 1]
        .iter()
        .all(|block| block.prompt().is_none())
}

#[cfg(test)]
mod tests;
