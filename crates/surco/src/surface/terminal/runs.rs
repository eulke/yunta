//! A field of runs, a row to each, its columns lined up down the field.

use crate::components::{Edge, RunLine, Runs};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, truncate, INDENT};

/// The fewest cells a row's sentence keeps before the furrow gives up
/// its column: what holds a run matters more than its picture.
const SAID_LEAST: usize = 16;

/// The cells a field's columns take on one line.
struct Columns {
    handle: usize,
    name: usize,
    furrow: usize,
    said: usize,
    edge: usize,
}

impl Runs {
    /// Each run on a row, its handle, its name, its furrow, what holds it
    /// and its edge each in a column of their own.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let columns = self.columns(look.width.cells());
        self.rows
            .iter()
            .map(|row| row.line(&columns, look))
            .collect()
    }

    /// The columns on a line of `width` cells: a handle as wide as the
    /// widest, the names up to a quarter of the line, two cells a node of
    /// furrow up to a fifth, the edge whole at the far end, and what holds
    /// each run in what is left — the furrow first to go when that is too
    /// little.
    fn columns(&self, width: usize) -> Columns {
        let widest = |cells: fn(&RunLine) -> usize| self.rows.iter().map(cells).max().unwrap_or(0);
        let handle = widest(|row| cell_width(&row.handle));
        let name =
            widest(|row| cell_width(&row.workflow) + 3 + cell_width(&row.mode)).min(width / 4);
        let edge = widest(|row| cell_width(row.edge.text()));
        let mut furrow = (widest(|row| row.furrow.len()) * 2).min(width / 5);
        // The margin, the mark and its space, and two cells between columns.
        let taken = |furrow: usize| {
            cell_width(INDENT)
                + 2
                + handle
                + 2
                + name
                + 2
                + furrow
                + usize::from(furrow > 0) * 2
                + 2
                + edge
        };
        if width.saturating_sub(taken(furrow)) < SAID_LEAST {
            furrow = 0;
        }
        Columns {
            handle,
            name,
            furrow,
            said: width.saturating_sub(taken(furrow)),
            edge,
        }
    }
}

impl RunLine {
    fn line(&self, columns: &Columns, look: &Look) -> Line {
        let mark = self.standing.mark();
        let name = truncate(
            &format!("{} ({})", self.workflow, self.mode),
            columns.name,
            look.glyphs,
        );
        let held = |on: bool, glyph: char| match on {
            true => Line::new().push(Tone::Strong, glyph.to_string()),
            false => Line::new().plain(" "),
        };
        let mut line = Line::new()
            .then(held(self.pointed, look.glyphs.pointer()))
            .then(held(self.marked, look.glyphs.picked()))
            .push(Tone::of(mark), look.glyphs.mark(mark).to_string())
            .plain(" ")
            .push(
                Tone::Strong,
                format!("{:<width$}", self.handle, width = columns.handle),
            )
            .plain("  ")
            .push(Tone::Muted, name)
            .plain("  ");
        if columns.furrow > 0 {
            let furrow = super::page::strokes(&self.furrow, columns.furrow, false, look);
            let drawn = cell_width(&furrow.text());
            line = line
                .then(furrow)
                .plain(" ".repeat(columns.furrow - drawn + 2));
        }
        let edge = match &self.edge {
            Edge::Age(age) => Line::new().push(Tone::Muted, age.as_str()),
            Edge::Command(command) => Line::new().push(Tone::Strong, command.as_str()),
        };
        let room = columns.edge - cell_width(self.edge.text());
        line.plain(truncate(&self.said, columns.said, look.glyphs))
            .plain(" ".repeat(2 + room))
            .then(edge)
    }
}

#[cfg(test)]
mod tests;
