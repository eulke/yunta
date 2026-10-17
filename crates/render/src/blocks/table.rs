//! Rows under named columns: each row with its mark, each column as wide
//! as what it holds, and the one column that holds what the line leaves.

use super::Drawn;
use crate::ink::{Line, Tone};
use crate::{cell_width, id_column, middle_cut, truncate, Look, Mark, INDENT, STATE_WIDTH};

/// Rows of cells under named columns.
///
/// A file heads each column with its title. A terminal draws no header:
/// a row is read by its mark and its id, and each cell says what it is.
pub struct Table {
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
}

/// One column: what a file heads it with, and what it holds.
pub struct Column {
    pub title: &'static str,
    pub holds: Holds,
}

/// What a column holds, which is how it is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holds {
    /// Where the row stands, in the state vocabulary: beside the row's
    /// mark, as wide as the vocabulary's widest word.
    State,
    /// What a reader types next: as wide as the widest, up to a third of
    /// the line. One cut to share it is cut in its middle, where siblings
    /// that share a prefix or a suffix differ.
    Id,
    /// Words read whole, as wide as the widest of them.
    Words,
    /// What the line leaves once every other column is whole: a cell
    /// longer than that is cut at its end.
    Rest,
}

/// One row: its mark, when it has one, and a cell for each column.
pub struct Row {
    pub mark: Option<Mark>,
    pub cells: Vec<Cell>,
}

/// What one cell says, and the role it plays.
pub struct Cell {
    pub text: String,
    pub tone: Tone,
}

impl Cell {
    /// A cell in the plain tone.
    pub fn plain(text: impl Into<String>) -> Self {
        Cell::toned(Tone::Plain, text)
    }

    /// A cell in `tone`.
    pub fn toned(tone: Tone, text: impl Into<String>) -> Self {
        Cell {
            text: text.into(),
            tone,
        }
    }
}

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
                Holds::State => STATE_WIDTH,
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

impl Drawn for Table {
    fn lines(&self, look: &Look) -> Vec<Line> {
        let widths = self.widths(look.width.cells());
        self.rows
            .iter()
            .map(|row| self.row(row, &widths, look))
            .collect()
    }
}
