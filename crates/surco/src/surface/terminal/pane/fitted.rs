//! See [`super`]. A region that holds neither a prompt nor a feed, in a pane too short for
//! all of it: where it says itself in fewer rows, the first of those that fits stands in its
//! place; where none fits, all of it stands, to scroll. Standing close, as a page does on a
//! screen short of rows, each labelled value and each checklist row stands on its own row,
//! cut at the edge.

use super::{closed_up, Spacing, Terminal};
use crate::doc::Block;
use crate::line::Line;

impl Terminal {
    /// The lines of `blocks`, a document's, in a pane `rows` high spaced by `spacing`: whole
    /// where they fit, else the first of `abridged`, what they say in fewer rows, that fits;
    /// whole again where none does, to scroll.
    pub(super) fn fitted(
        &self,
        blocks: &[Block],
        abridged: &[Vec<Block>],
        rows: usize,
        spacing: Spacing,
    ) -> Vec<Line> {
        let drawn = |blocks: &[Block]| match spacing {
            Spacing::Close => closed_up(self.values_on_their_row().in_rhythm(blocks), spacing),
            Spacing::Aired => self.in_rhythm(blocks),
        };
        let whole = drawn(blocks);
        if whole.len() <= rows {
            return whole;
        }
        let shorter = abridged
            .iter()
            .map(|blocks| drawn(blocks))
            .find(|lines| lines.len() <= rows);
        shorter.unwrap_or(whole)
    }
}

#[cfg(test)]
mod tests;
