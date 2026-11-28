//! The name in block letters at a terminal: its two rows, each centred in the cells the look
//! gives it, in the strong tone. Where the room either side cannot split evenly, the odd cell
//! stands before the row; the letters' rows are as wide as each other, so they never move
//! apart.
//!
//! In ASCII the name stands in its own letters on the first row and the second stays blank,
//! the glyph set's form of it: a screen lays out what stands under it on the same rows in
//! either set. In fewer cells than its letters take it draws nothing: cut, the letters spell
//! no name, and the trail names it.

use super::Terminal;
use crate::line::{Line, Tone};
use crate::width::cell_width;

impl Terminal {
    /// The name in block letters across this look's width, a line a row, each ending after
    /// its last letter; none where the letters do not fit.
    pub fn wordmark(&self) -> Vec<Line> {
        let cells = self.look.width.cells();
        let rows = self.look.glyphs.wordmark();
        if rows.iter().any(|row| cell_width(row) > cells) {
            return Vec::new();
        }
        rows.iter()
            .map(|row| match row.is_empty() {
                true => Line::new(),
                false => {
                    let before = (cells - cell_width(row)).div_ceil(2);
                    Line::new()
                        .plain(" ".repeat(before))
                        .push(Tone::Strong, *row)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
