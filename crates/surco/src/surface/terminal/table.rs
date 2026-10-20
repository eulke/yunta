//! Rows under named columns, each column as wide as what it holds.

use crate::components::{Holds, Row, Table};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, id_column, middle_cut, truncate, INDENT};
use crate::words::Standing;

impl Table {
    /// Each column's width on a line of `width` cells, and whether rows
    /// leave room for a mark.
    fn widths(&self, width: usize) -> Vec<usize> {
        let held = |at: usize| {
            self.rows
                .iter()
                .filter_map(move |row| row.cells.get(at))
                .map(|cell| cell.text.as_str())
        };
        let mut widths: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .map(|(at, column)| match column.holds {
                Holds::State => Standing::WIDEST,
                Holds::Id => id_column(held(at)).min(width / 3),
                Holds::Words => id_column(held(at)),
                Holds::Rest => 0,
            })
            .collect();
        let gaps: usize = (1..self.columns.len()).map(|at| self.gap(at)).sum();
        let fixed = cell_width(INDENT) + self.mark_width() + gaps + widths.iter().sum::<usize>();
        for (at, column) in self.columns.iter().enumerate() {
            if column.holds == Holds::Rest {
                widths[at] = width.saturating_sub(fixed);
            }
        }
        // A line too narrow for every column whole cuts the widest words
        // first, one cell at a time, until it fits.
        for _ in 0..fixed.saturating_sub(width) {
            let widest = (0..widths.len())
                .filter(|at| self.columns[*at].holds == Holds::Words)
                .max_by_key(|at| widths[*at])
                .filter(|at| widths[*at] > 0);
            let Some(widest) = widest else {
                break;
            };
            widths[widest] -= 1;
        }
        widths
    }

    /// The cells a row's mark takes, and the space after it: none when no
    /// row has one.
    fn mark_width(&self) -> usize {
        match self.rows.iter().any(|row| row.mark.is_some()) {
            true => 2,
            false => 0,
        }
    }

    /// The space before column `at`: one after a state, which reads with
    /// its mark as one phrase, two between any others.
    fn gap(&self, at: usize) -> usize {
        match self
            .columns
            .get(at.wrapping_sub(1))
            .map(|column| column.holds)
        {
            Some(Holds::State) => 1,
            _ => 2,
        }
    }

    /// `row` laid out in `widths`, ending at its last cell with anything
    /// to say.
    fn row(&self, row: &Row, widths: &[usize], look: &Look) -> Line {
        let mut line = Line::new().plain(INDENT);
        if self.mark_width() > 0 {
            line = match row.mark {
                Some(mark) => line.push(Tone::of(mark), look.glyphs.mark(mark).to_string()),
                None => line.plain(" "),
            }
            .plain(" ");
        }
        let Some(last) = row
            .cells
            .iter()
            .zip(widths)
            .rposition(|(cell, width)| *width > 0 && !cell.text.trim().is_empty())
        else {
            return line;
        };
        for (at, (cell, column)) in row
            .cells
            .iter()
            .zip(&self.columns)
            .enumerate()
            .take(last + 1)
        {
            if at > 0 {
                line = line.plain(" ".repeat(self.gap(at)));
            }
            let width = widths[at];
            let fitted = match column.holds {
                Holds::Id => middle_cut(&cell.text, width, look.glyphs),
                Holds::State | Holds::Words | Holds::Rest => {
                    truncate(&cell.text, width, look.glyphs)
                }
            };
            let fitted = match at == last {
                true => fitted.trim_end().to_string(),
                false => fitted,
            };
            line = line.push(cell.tone, fitted);
        }
        line
    }
}

impl Table {
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let widths = self.widths(look.width.cells());
        self.rows
            .iter()
            .map(|row| self.row(row, &widths, look))
            .collect()
    }
}
