//! A run's nodes, one to a row: the mark and word for where each stands,
//! its id, and a note.

use super::Block;
use crate::render::ink::{Line, Tone};
use crate::render::{cell_width, id_column, middle_cut, truncate, Look, Mark, INDENT, STATE_WIDTH};

/// One node's row.
pub(crate) struct NodeRow {
    pub(crate) mark: Mark,
    /// The word for where the node stands, in the one vocabulary.
    pub(crate) word: &'static str,
    pub(crate) id: String,
    /// What the node said or what stopped it, in one line.
    pub(crate) note: String,
}

/// Every node of a run in a table whose id column is as wide as its
/// longest id, up to a third of the line: an id is what a reader types
/// next, and one cut to share a column is cut in its middle, where two
/// fan-out siblings never differ.
pub(crate) struct NodeTable {
    pub(crate) rows: Vec<NodeRow>,
}

impl Block for NodeTable {
    fn lines(&self, look: &Look) -> Vec<Line> {
        let width = look.width.cells();
        let column = id_column(self.rows.iter().map(|row| row.id.as_str())).min(width / 3);
        let fixed = cell_width(INDENT) + 2 + STATE_WIDTH + 1 + column + 2;
        let room = width.saturating_sub(fixed);
        self.rows
            .iter()
            .map(|row| {
                let tone = Tone::of(row.mark);
                let id = middle_cut(&row.id, column, look.glyphs);
                let line = Line::new()
                    .plain(INDENT)
                    .push(tone, look.glyphs.mark(row.mark).to_string())
                    .plain(" ")
                    .push(tone, format!("{:<STATE_WIDTH$}", row.word))
                    .plain(" ");
                match row.note.trim().is_empty() {
                    true => line.plain(id.trim_end()),
                    false => line.plain(id).plain("  ").push(
                        Tone::Muted,
                        truncate(&row.note, room, look.glyphs).trim_end(),
                    ),
                }
            })
            .collect()
    }
}
