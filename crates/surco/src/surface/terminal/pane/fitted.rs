//! See [`super`]. A region that holds neither a prompt nor a feed, in a pane too short for
//! all of it: where it says itself in fewer rows, the first of those that fits stands in its
//! place; where none fits, all of it stands, to scroll.

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
        let whole = closed_up(self.in_rhythm(blocks), spacing);
        if whole.len() <= rows {
            return whole;
        }
        let shorter = abridged
            .iter()
            .map(|blocks| closed_up(self.in_rhythm(blocks), spacing))
            .find(|lines| lines.len() <= rows);
        shorter.unwrap_or(whole)
    }
}

#[cfg(test)]
mod tests;
