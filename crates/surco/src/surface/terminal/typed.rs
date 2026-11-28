//! The row a person types on, at a terminal: a box of three rows across the cells it is
//! given, its edges muted in round corners, the words strong one cell inside its
//! uprights. The box holds as much of the words around the cursor as fits, so the end of
//! a line being typed stays in sight and the cursor is never drawn off the row; the
//! cursor stands on the middle row.
//!
//! It is the one drawing of a line asked of a person: a typed answer, the words a card's
//! option is said with, a round's typed answer and the words typed at a desk's foot each
//! draw their line through it, a desk's with what is asked of the words faint in the box
//! while nothing is typed. Where
//! a line is written rather than held, as Markdown and a page write it, the row's prompt
//! opens it instead. A list narrowed by typing is not a line asked: what was typed stands
//! over the choices it leaves, on a row of the list's own, and no box is drawn for it.

use super::Placed;
use crate::components::TypedRow;
use crate::diagram::{Shape, Stroke};
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::width::cell_width;

/// The cells between an upright and the words: they never touch the edge they stand in.
const INSIDE: usize = 1;

/// The cells each side of the box takes: its upright, and the cell inside it.
const SIDE: usize = 1 + INSIDE;

/// The cell the cursor needs past the last character of the words.
const CURSOR_CELL: usize = 1;

impl TypedRow {
    /// The row boxed `cells` wide: the top edge, the words between the uprights, the
    /// bottom edge; with where the cursor stands, on the middle row after the words
    /// before it (the row counted from the top edge, and the cell). While nothing is
    /// typed, a `placeholder` stands faint where the words go, cut at the box, the cursor
    /// on its first cell.
    pub(super) fn drawn(&self, cells: usize, glyphs: Glyphs, placeholder: Option<&str>) -> Placed {
        let [top_left, top_right, bottom_left, bottom_right] = glyphs.corners(Shape::Round);
        let level = glyphs
            .joint(false, false, true, true, Stroke::Open)
            .to_string()
            .repeat(cells.saturating_sub(2));
        let upright = glyphs
            .joint(true, true, false, false, Stroke::Open)
            .to_string();
        let inside = cells.saturating_sub(2 * SIDE);
        let (before, after) = self.fitted(inside.saturating_sub(CURSOR_CELL));
        let cursor = SIDE + cell_width(&before);
        let words = match (placeholder, self.is_empty()) {
            (Some(placeholder), true) => Line::new()
                .push(Tone::Faint, placeholder)
                .cut(inside, glyphs),
            _ => Line::new().push(Tone::Strong, format!("{before}{after}")),
        };
        let pad = inside.saturating_sub(cell_width(&words.text()));
        let middle = Line::new()
            .push(Tone::Muted, upright.as_str())
            .plain(" ".repeat(INSIDE))
            .then(words)
            .plain(" ".repeat(pad + INSIDE))
            .push(Tone::Muted, upright);
        Placed {
            lines: vec![
                Line::new().push(Tone::Muted, format!("{top_left}{level}{top_right}")),
                middle,
                Line::new().push(Tone::Muted, format!("{bottom_left}{level}{bottom_right}")),
            ],
            cursor: Some((1, cursor)),
            laid: None,
        }
    }

    /// The row boxed `cells` wide as [`TypedRow::drawn`] boxes it, nothing typed standing
    /// empty, `title` in its top edge a cell in from its corner where the edge holds it: a
    /// line typed in place under a row names what it takes in its edge rather than over it.
    pub(super) fn drawn_titled(&self, title: &str, cells: usize, glyphs: Glyphs) -> Placed {
        let mut placed = self.drawn(cells, glyphs, None);
        let [top_left, top_right, ..] = glyphs.corners(Shape::Round);
        let level = glyphs.joint(false, false, true, true, Stroke::Open);
        let titled = format!("{top_left} {title} ");
        let left = cells.saturating_sub(cell_width(&titled) + cell_width(&top_right.to_string()));
        if left > 0 {
            let edge = format!("{titled}{}{top_right}", level.to_string().repeat(left));
            placed.lines[0] = Line::new().push(Tone::Muted, edge);
        }
        placed
    }

    /// What `room` cells show of the words being typed, split at the cursor: the
    /// characters before it and the ones after it.
    ///
    /// The cells are filled backwards from the cursor first and forwards with what is
    /// left, so the end of a line being typed stays in sight and the cursor is never
    /// drawn off the row.
    fn fitted(&self, room: usize) -> (String, String) {
        let room = room.max(1);
        let (behind, taken) = fitting(self.before.chars().rev(), room);
        let (ahead, _) = fitting(self.after.chars(), room.saturating_sub(taken));
        (
            behind.into_iter().rev().collect(),
            ahead.into_iter().collect(),
        )
    }
}

/// The longest run of `characters` that fits `room` cells, taken in the order they
/// arrive, and the cells it takes.
fn fitting(characters: impl Iterator<Item = char>, room: usize) -> (Vec<char>, usize) {
    let mut taken = 0;
    let mut kept = Vec::new();
    for character in characters {
        let mut buffer = [0u8; 4];
        let width = cell_width(character.encode_utf8(&mut buffer));
        if taken + width > room {
            break;
        }
        taken += width;
        kept.push(character);
    }
    (kept, taken)
}

#[cfg(test)]
mod tests;
