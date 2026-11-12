//! What a surface needs to know about the stream it is drawn on: the
//! characters it may draw, the paint it gets, how many cells a line may
//! take on it, and whether what moves on it turns.
//!
//! The width is measured, not assumed: a terminal says how wide it is,
//! and a reader who wants another width says so with `COLUMNS`, which
//! wins over what was measured. Either is held between a floor and a
//! ceiling. Below the floor a column of names and the words beside it
//! stop fitting on one row, and a surface laid out any narrower would be
//! cut into words nobody can read; the terminal wraps a line laid out at
//! the floor instead. Above the ceiling a line is too long for the eye to come
//! back to the start of the next one. Off a terminal, with no width
//! asked for, a line is laid out at [`LINE_WIDTH`]: the width it still
//! has to survive once it is pasted into a review, an issue, a log.
//!
//! Two widths pass the floor. A row redrawn in place never passes the
//! terminal's edge: past it the row wraps onto the row below, and the next
//! redraw clears one row too few. A pane is drawn at exactly its cells: its
//! own edge bounds it, so a block drawn there takes those cells and no more.

use crate::glyphs::Glyphs;
use crate::motion::Motion;
use crate::paint::Paint;
use crate::width::LINE_WIDTH;

/// How one stream's lines are drawn: the characters they are drawn
/// with, the paint they get, the cells they may take, and whether what
/// moves on them turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub glyphs: Glyphs,
    pub paint: Paint,
    pub width: Width,
    /// Whether a running mark's spin and a ribbon's plow turn with the
    /// events that arrive: still for a reader who asked a view to stand
    /// still, and for a page that is read rather than watched.
    pub motion: Motion,
}

impl Look {
    /// The look of a line with nothing but its words: ASCII, unpainted,
    /// at the width a line has off a terminal, standing still — what a
    /// test compares a surface against, and what a reader with no terminal
    /// gets.
    pub fn plain() -> Self {
        Look {
            glyphs: Glyphs::Ascii,
            paint: Paint::Plain,
            width: Width::of(None, None),
            motion: Motion::Still,
        }
    }

    /// The look of a terminal a golden is rendered for.
    #[cfg(any(test, feature = "golden"))]
    pub fn of(environment: &crate::golden::Environment) -> Self {
        Look {
            glyphs: match environment.unicode {
                true => Glyphs::Unicode,
                false => Glyphs::Ascii,
            },
            paint: environment.paint,
            width: Width::of(Some(environment.width), None),
            motion: Motion::Moves,
        }
    }
}

/// The cells one line on one stream may take, and what becomes of a line
/// that passes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Width {
    cells: usize,
    edge: Edge,
}

/// What becomes of a line that passes its width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    /// A stream's terminal wraps it, and a copy takes a word it wraps whole.
    Wraps,
    /// A pane's edge cuts it, so a surface goes on in the row under it.
    Cuts,
}

impl Width {
    /// Narrower than this, a name column and what sits beside it no
    /// longer share a row.
    const FLOOR: usize = 60;

    /// Wider than this, a line is too long to read in one sweep.
    const CEILING: usize = 120;

    /// The width a line gets from what the terminal `measured` and the
    /// `columns` a reader asked for: the asked-for width over the
    /// measured one, held between the floor and the ceiling, and
    /// [`LINE_WIDTH`] when neither says anything.
    pub fn of(measured: Option<usize>, columns: Option<usize>) -> Self {
        let cells = match columns.or(measured) {
            Some(cells) => cells.clamp(Self::FLOOR, Self::CEILING),
            None => LINE_WIDTH,
        };
        Width {
            cells,
            edge: Edge::Wraps,
        }
    }

    /// This width, never past a terminal `edge` cells wide: for a row
    /// redrawn in place, which tears if it wraps.
    pub fn within(self, edge: usize) -> Self {
        Width {
            cells: self.cells.min(edge),
            ..self
        }
    }

    /// The width a row redrawn in place on a terminal `edge` cells wide
    /// gets, `columns` being the width its reader asked for, if any.
    pub fn row(edge: usize, columns: Option<usize>) -> Self {
        Self::of(Some(edge), columns).within(edge)
    }

    /// Exactly `cells`, with no floor and no ceiling: for a pane, whose own
    /// edge bounds it, and for what lays out its own columns across a
    /// terminal and holds each to a line's measure itself.
    pub fn exact(cells: usize) -> Self {
        Width {
            cells,
            edge: Edge::Cuts,
        }
    }

    pub fn cells(self) -> usize {
        self.cells
    }

    /// Whether the edge of these cells cuts what passes it, as a pane's
    /// does: a line drawn there goes on in the row under it rather than
    /// pass it.
    pub fn cuts(self) -> bool {
        self.edge == Edge::Cuts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminal_narrower_than_the_floor_is_drawn_at_the_floor() {
        assert_eq!(Width::of(Some(40), None).cells(), Width::FLOOR);
    }

    #[test]
    fn a_terminal_wider_than_the_ceiling_is_drawn_at_the_ceiling() {
        assert_eq!(Width::of(Some(300), None).cells(), Width::CEILING);
        assert_eq!(Width::of(Some(100), None).cells(), 100);
    }

    #[test]
    fn columns_overrides_the_measured_width_within_bounds() {
        assert_eq!(Width::of(Some(80), Some(100)).cells(), 100);
        assert_eq!(Width::of(Some(80), Some(20)).cells(), Width::FLOOR);
        assert_eq!(Width::of(None, Some(500)).cells(), Width::CEILING);
    }

    #[test]
    fn off_a_terminal_a_line_is_eighty_cells() {
        assert_eq!(Width::of(None, None).cells(), 80);
    }

    #[test]
    fn a_row_redrawn_in_place_never_passes_the_terminals_edge() {
        assert_eq!(Width::row(40, None).cells(), 40);
        assert_eq!(Width::row(100, None).cells(), 100);
        assert_eq!(Width::row(200, None).cells(), Width::CEILING);
    }
}
