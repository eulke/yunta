//! A page of cells a diagram is drawn on: text where a box says
//! something, and lines that remember which ways they run through each
//! cell, so where two meet the cell draws the joint they make.

use super::Stroke;
use crate::Glyphs;

/// The ways a line runs through a cell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Ways {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

/// What a cell holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cell {
    Empty,
    /// A character written as it is: a label's, a corner's, an arrowhead.
    Fixed(char),
    /// Lines through the cell, drawn as the joint they make, in the
    /// stroke of the link that drew them.
    Lines(Ways, Stroke),
}

/// A page of `width` by `height` cells.
pub(crate) struct Grid {
    width: usize,
    cells: Vec<Vec<Cell>>,
}

impl Grid {
    pub(crate) fn new(width: usize, height: usize) -> Self {
        Grid {
            width,
            cells: vec![vec![Cell::Empty; width]; height],
        }
    }

    /// Whether the cell at `x`, `y` holds nothing.
    pub(crate) fn free(&self, x: usize, y: usize) -> bool {
        self.cells
            .get(y)
            .and_then(|row| row.get(x))
            .is_some_and(|cell| *cell == Cell::Empty)
    }

    /// `ch` at `x`, `y`, whatever the cell held.
    pub(crate) fn put(&mut self, x: usize, y: usize, ch: char) {
        if let Some(cell) = self.cells.get_mut(y).and_then(|row| row.get_mut(x)) {
            *cell = Cell::Fixed(ch);
        }
    }

    /// `text` from `x` on row `y`, a character to a cell.
    pub(crate) fn write(&mut self, x: usize, y: usize, text: &str) {
        for (at, ch) in text.chars().enumerate() {
            self.put(x + at, y, ch);
        }
    }

    /// Whether `text` fits from `x` on row `y` over cells that hold
    /// nothing.
    pub(crate) fn room(&self, x: usize, y: usize, text: &str) -> bool {
        x + text.chars().count() <= self.width
            && (0..text.chars().count()).all(|at| self.free(x + at, y))
    }

    /// Whether `text` fits from `x` on row `y` over cells that hold
    /// nothing or only lines — a label written along its own link.
    pub(crate) fn lines_only(&self, x: usize, y: usize, text: &str) -> bool {
        x + text.chars().count() <= self.width
            && (0..text.chars().count()).all(|at| {
                self.cells
                    .get(y)
                    .and_then(|row| row.get(x + at))
                    .is_some_and(|cell| !matches!(cell, Cell::Fixed(_)))
            })
    }

    /// A line through the cell at `x`, `y` running the ways `ways` says,
    /// joined to whatever lines already run through it. A cell holding
    /// text keeps it.
    fn through(&mut self, x: usize, y: usize, ways: Ways, stroke: Stroke) {
        let Some(cell) = self.cells.get_mut(y).and_then(|row| row.get_mut(x)) else {
            return;
        };
        *cell = match *cell {
            Cell::Empty => Cell::Lines(ways, stroke),
            // Two links through one cell draw it plain: neither one's
            // stroke says what the joint is.
            Cell::Lines(was, drawn) => Cell::Lines(
                Ways {
                    up: was.up || ways.up,
                    down: was.down || ways.down,
                    left: was.left || ways.left,
                    right: was.right || ways.right,
                },
                if drawn == stroke {
                    stroke
                } else {
                    Stroke::Arrow
                },
            ),
            fixed @ Cell::Fixed(_) => fixed,
        };
    }

    /// A line down column `x` from row `top` to row `bottom`, both ends
    /// included: each end runs only toward the other.
    pub(crate) fn vertical(&mut self, x: usize, top: usize, bottom: usize, stroke: Stroke) {
        let (top, bottom) = (top.min(bottom), top.max(bottom));
        for y in top..=bottom {
            self.through(
                x,
                y,
                Ways {
                    up: y > top,
                    down: y < bottom,
                    ..Ways::default()
                },
                stroke,
            );
        }
    }

    /// A line along row `y` from column `left` to column `right`, both
    /// ends included.
    pub(crate) fn horizontal(&mut self, y: usize, left: usize, right: usize, stroke: Stroke) {
        let (left, right) = (left.min(right), left.max(right));
        for x in left..=right {
            self.through(
                x,
                y,
                Ways {
                    left: x > left,
                    right: x < right,
                    ..Ways::default()
                },
                stroke,
            );
        }
    }

    /// Each row as text, its trailing space gone.
    pub(crate) fn rows(&self, glyphs: Glyphs) -> Vec<String> {
        self.cells
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| match cell {
                        Cell::Empty => ' ',
                        Cell::Fixed(ch) => *ch,
                        Cell::Lines(ways, stroke) => {
                            glyphs.joint(ways.up, ways.down, ways.left, ways.right, *stroke)
                        }
                    })
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }
}
