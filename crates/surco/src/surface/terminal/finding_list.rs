//! Each finding on its row: the pointer's cells, its mark in its tone, its severity's word
//! muted in a column as wide as the widest word on the list, and its title, cut at the row's
//! end; the answer of an answered one muted under its title, cut there too. A row stays one
//! row, so a list keeps its height whatever its titles say.

use crate::components::{FindingList, FindingRow};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::cell_width;

impl FindingList {
    /// Each finding's row, and under an answered one its answer, at the look's width. The
    /// pointed row leads with the pointer and stands on the selection, its title strong, and
    /// its answer stands on the selection with it: the two rows are one finding.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let column = self
            .rows
            .iter()
            .map(|row| cell_width(&row.severity))
            .max()
            .unwrap_or(0);
        self.rows
            .iter()
            .enumerate()
            .flat_map(|(at, row)| row.lines(self.pointed == Some(at), column, look))
            .collect()
    }
}

impl FindingRow {
    /// This finding's row, its severity's word padded to `column`, and its answer under its
    /// title where it has one, each cut at the look's width.
    fn lines(&self, pointed: bool, column: usize, look: &Look) -> Vec<Line> {
        let glyphs = look.glyphs;
        let cells = look.width.cells();
        let mark = self.mark();
        let title_tone = match pointed {
            true => Tone::Strong,
            false => Tone::Plain,
        };
        let pad = " ".repeat(column.saturating_sub(cell_width(&self.severity)));
        let lead = Line::pointed(pointed, glyphs)
            .push(Tone::of(mark), glyphs.mark(mark).to_string())
            .plain(" ")
            .push(Tone::Muted, format!("{}{pad}", self.severity))
            .plain(" ");
        let under = " ".repeat(cell_width(&lead.text()));
        let fill = lead.fill();
        let row = lead
            .push(title_tone, self.title.as_str())
            .cut(cells, glyphs);
        let answer = self.answer.as_deref().map(|answer| {
            let line = Line::new()
                .plain(under.as_str())
                .push(Tone::Muted, answer)
                .cut(cells, glyphs);
            match fill {
                Some(fill) => line.filled(fill),
                None => line,
            }
        });
        std::iter::once(row).chain(answer).collect()
    }
}
