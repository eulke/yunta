//! Rows under named columns: each row with its mark, each column as wide
//! as what it holds, and the one column that holds what the line leaves.

use crate::glyphs::Mark;
use crate::line::Tone;

/// Rows of cells under named columns.
///
/// A file heads each column with its title. A terminal heads only a table
/// that holds figures: a row is read by its mark and its id, and a word
/// says what it is, but a figure needs saying what it counts.
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
    /// A figure read against the ones above and below it — a duration, an
    /// amount: as wide as the widest, flush right, and never cut, since a
    /// cut figure reads as another one.
    Figure,
    /// What the line leaves once every other column is whole: a cell
    /// longer than that is cut at its end.
    Rest,
    /// What the line leaves, like [`Holds::Rest`], read as words a cut
    /// would lose: a longer cell wraps under itself and is never cut. The
    /// last column of its table.
    Prose,
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
    /// Where in `text` the id itself stands, for a cell of an id column that says more than
    /// its id: a surface that sets an id apart sets apart this and nothing else. `None` for
    /// a cell whose whole text is what its column holds.
    pub id: Option<std::ops::Range<usize>>,
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
            id: None,
        }
    }

    /// A cell of an id column: `id` after what comes `before` it, and before what comes `after`
    /// it, in the plain tone.
    pub fn around(before: String, id: &str, after: String) -> Self {
        let start = before.len();
        Cell {
            text: format!("{before}{id}{after}"),
            tone: Tone::Plain,
            id: Some(start..start + id.len()),
        }
    }
}
