//! How many cells a line may take on the stream it is written to.
//!
//! Measured, not assumed: a terminal says how wide it is, and a reader
//! who wants another width says so with `COLUMNS`, which wins over what
//! was measured. Either is held between a floor and a ceiling. Below the
//! floor a column of names and the words beside it stop fitting on one
//! row, and a surface laid out any narrower would be cut into words
//! nobody can read; the terminal wraps a line laid out at the floor
//! instead. Above the ceiling a line is too long for the eye to come
//! back to the start of the next one. Off a terminal, with no width
//! asked for, a line is laid out at [`LINE_WIDTH`]: the width it still
//! has to survive once it is pasted into a review, an issue, a log.
//!
//! A row redrawn in place is the one exception to the floor: past the
//! terminal's edge it wraps onto the row below, and the next redraw
//! clears one row too few.

use std::sync::OnceLock;

use dialoguer::console::Term;

use super::LINE_WIDTH;

/// The cells one line on one stream may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Width(usize);

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
    pub(crate) fn of(measured: Option<usize>, columns: Option<usize>) -> Self {
        match columns.or(measured) {
            Some(cells) => Width(cells.clamp(Self::FLOOR, Self::CEILING)),
            None => Width(LINE_WIDTH),
        }
    }

    /// This width, never past a terminal `edge` cells wide: for a row
    /// redrawn in place, which tears if it wraps.
    pub(crate) fn within(self, edge: usize) -> Self {
        Width(self.0.min(edge))
    }

    /// The width a row redrawn in place on a terminal `edge` cells wide
    /// gets, in this process.
    pub(crate) fn row(edge: usize) -> Self {
        Self::of(Some(edge), columns()).within(edge)
    }

    /// The width stdout gets in this process.
    pub(crate) fn stdout() -> Self {
        Self::of(measured(&Term::stdout()), columns())
    }

    pub(crate) fn cells(self) -> usize {
        self.0
    }
}

/// How wide `term` is, when it is a terminal at all.
fn measured(term: &Term) -> Option<usize> {
    term.size_checked().map(|(_, columns)| usize::from(columns))
}

static COLUMNS: OnceLock<Option<usize>> = OnceLock::new();

/// Fixes the width this process's reader asked for — `COLUMNS`, when it
/// names one. `main` calls it once, before anything is printed; a later
/// call changes nothing.
pub(crate) fn settle(columns: Option<usize>) {
    COLUMNS.get_or_init(|| columns);
}

/// The width this process's reader asked for, if any; none in a process
/// that never settled one — a unit test.
fn columns() -> Option<usize> {
    COLUMNS.get().copied().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminal_narrower_than_the_floor_is_drawn_at_the_floor() {
        assert_eq!(Width::of(Some(40), None), Width(Width::FLOOR));
    }

    #[test]
    fn a_terminal_wider_than_the_ceiling_is_drawn_at_the_ceiling() {
        assert_eq!(Width::of(Some(300), None), Width(Width::CEILING));
        assert_eq!(Width::of(Some(100), None), Width(100));
    }

    #[test]
    fn columns_overrides_the_measured_width_within_bounds() {
        assert_eq!(Width::of(Some(80), Some(100)), Width(100));
        assert_eq!(Width::of(Some(80), Some(20)), Width(Width::FLOOR));
        assert_eq!(Width::of(None, Some(500)), Width(Width::CEILING));
    }

    #[test]
    fn off_a_terminal_a_line_is_eighty_cells() {
        assert_eq!(Width::of(None, None).cells(), 80);
    }

    #[test]
    fn a_row_redrawn_in_place_never_passes_the_terminals_edge() {
        assert_eq!(Width::row(40), Width(40));
        assert_eq!(Width::row(100), Width(100));
        assert_eq!(Width::row(200), Width(Width::CEILING));
    }
}
