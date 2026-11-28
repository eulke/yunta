//! How a row a pointer moves along looks, each look once, by what the row is for: a row a
//! person picks from, as a list's, a palette's, a card's options and the field's runs are,
//! stands on the selection to its region's edge, its pointer in the needs-you tone, since what
//! is pointed at is what the person acts on; a row of a page read along a pointer, as a
//! checklist's checks and a setup's rows are, stands on no selection, its pointer strong, since
//! what it says is read where it stands. A row not pointed at keeps the pointer's cells blank,
//! so no row moves when the pointer does.

use super::{Fill, Line, Tone};
use crate::glyphs::Glyphs;
use crate::width::cell_width;

impl Line {
    /// How a row a person picks from opens, `on` the row pointed at or not, as a list's, a
    /// palette's and a card's options do: led by its pointer's cell ([`Line::pointer`]) and a
    /// space, standing on the selection to its region's edge ([`Line::on_the_selection`]); or
    /// by the blank cells they take, with no fill, so no row moves when the pointer does. The
    /// row's words follow. A row that is picked is one of two looks a row a pointer moves along
    /// has; the other is a row read along a pointer ([`Line::read_along`]). Not
    /// [`Glyphs::pointer`], the glyph alone.
    pub fn pointed(on: bool, glyphs: Glyphs) -> Line {
        Line::pointer(on, glyphs).plain(" ").on_the_selection(on)
    }

    /// The pointer's own cells on a row a person picks from: the pointer in the needs-you
    /// tone, since what is pointed at is what the person acts on, or as many blank cells. What a
    /// row that lays out its own columns after it opens with, as a run's row in a list does
    /// before its picked mark.
    pub fn pointer(on: bool, glyphs: Glyphs) -> Line {
        let pointer = glyphs.pointer();
        match on {
            true => Line::new().push(Tone::NeedsYou, pointer.to_string()),
            false => Line::new().plain(" ".repeat(cell_width(&pointer.to_string()))),
        }
    }

    /// This row on the selection to its region's edge where it is the row a person picks from
    /// that the pointer stands on (`on`), and as it is otherwise.
    pub fn on_the_selection(self, on: bool) -> Line {
        match on {
            true => self.filled(Fill::Selection),
            false => self,
        }
    }

    /// How a row of a page read along a pointer opens, `on` the row pointed at or not, as a
    /// checklist's checks and a setup's rows do: led by the pointer, strong, and a space, on no
    /// selection, since what the row says is read where it stands, not picked; or by the blank
    /// cells they take. A surface finds the row such a pointer stands on by the pointer that
    /// leads it. Not [`Line::pointed`], a row a person picks from.
    pub fn read_along(on: bool, glyphs: Glyphs) -> Line {
        let pointer = glyphs.pointer().to_string();
        match on {
            true => Line::new().push(Tone::Strong, pointer).plain(" "),
            false => Line::new()
                .plain(" ".repeat(cell_width(&pointer)))
                .plain(" "),
        }
    }
}
