//! See [`super`]. A region whose words are set to a measure, in a pane wider than it: its
//! words drawn on a terminal as wide as the measure, by the same rules as any region's, then
//! moved right so the column stands centred in the pane, the odd cell at the left as the
//! name in block letters stands, or left at the pane's left edge where the measure says so;
//! the cursor of a prompt among them moved with them. The
//! terminal lends a line said across the column the pane's width (`Width::in_a_pane`), and
//! such a line, wider than the column, stands centred over it, the odd cell at the left too.
//! A pane no wider than the measure draws the words across its width, as a region with no
//! measure.

use super::{Paned, Terminal};
use crate::components::Measure;
use crate::look::Width;
use crate::width::cell_width;

/// The column a region's words stand in where they are set to a measure narrower than the
/// pane: the terminal they are drawn on, as wide as the measure, and the cells it stands in
/// from the pane's left edge. Not the pane, which is as wide as the region.
pub(super) struct Column {
    pub(super) words: Terminal,
    from: usize,
    cells: usize,
}

impl Terminal {
    /// The column words set to `measure` stand in on this pane, where the pane is wider than
    /// the measure: none where it is not, and none for words set to no measure.
    pub(super) fn column(&self, measure: Option<Measure>) -> Option<Column> {
        let pane = self.look.width.cells();
        let measure = measure.filter(|measure| measure.cells < pane)?;
        let cells = measure.cells;
        let words = self.at_width(Width::exact(cells).in_a_pane(pane));
        let from = match measure.centred {
            true => (pane - cells).div_ceil(2),
            false => 0,
        };
        Some(Column { words, from, cells })
    }
}

impl Column {
    /// `paned`, drawn on the column's terminal, placed in the pane: each row moved right to
    /// the column's first cell, a row said across the column wider than it centred over it,
    /// a blank row left blank, and the cursor moved with them.
    pub(super) fn placed(&self, paned: Paned) -> Paned {
        let lines = paned
            .lines
            .into_iter()
            .map(|line| {
                let over = cell_width(&line.text()).saturating_sub(self.cells);
                let margin = " ".repeat(self.from.saturating_sub(over.div_ceil(2)));
                match line.text().is_empty() {
                    true => line,
                    false => line.under(&margin),
                }
            })
            .collect();
        Paned {
            lines,
            cursor: paned.cursor.map(|(row, cell)| (row, cell + self.from)),
            laid: paned.laid,
        }
    }
}
