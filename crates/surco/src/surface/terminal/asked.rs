//! A line asked and a list picked from, at a terminal: the question and the row an
//! answer is typed on, with the cursor on it; the list what was typed leaves, with
//! the keys under it, in the rows it is given.

use super::card::{Placed, CURSOR_CELL};
use super::narrowed;
use crate::components::{ChoiceList, TypedAnswer};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::cell_width;

impl TypedAnswer {
    /// The question, and the row the words are typed on, showing the part of them
    /// around the cursor that fits, with the cursor on it.
    pub fn drawn(&self, look: &Look) -> Placed {
        let prompt = &self.typed.prompt;
        let room = look
            .width
            .cells()
            .saturating_sub(cell_width(prompt) + CURSOR_CELL);
        let (before, after) = self.typed.fitted(room);
        let cursor = (1, cell_width(prompt) + cell_width(&before));
        Placed {
            lines: vec![
                self.asked.clone(),
                Line::new()
                    .plain(prompt.as_str())
                    .plain(before)
                    .plain(after),
            ],
            cursor: Some(cursor),
            laid: None,
        }
    }
}

impl ChoiceList {
    /// The list in at most `rows` rows: what was typed over it and the options it leaves,
    /// in the rows the keys under it leave. A key line that wraps takes from the list.
    pub fn drawn(&self, look: &Look, rows: usize) -> Placed {
        let cells = look.width.cells();
        let keys = Line::new()
            .push(Tone::Muted, self.legend.spelled(look.glyphs))
            .wrap(cells);
        let room = rows.saturating_sub(keys.len());
        let mut lines = narrowed(
            self.choices.as_ref(),
            self.filter.as_deref(),
            look,
            cells,
            room,
        );
        lines.extend(keys);
        Placed {
            lines,
            cursor: None,
            laid: None,
        }
    }
}
