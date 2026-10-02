//! Labelled facts, one to a row, the label in a column of its own.

use super::Block;
use crate::ink::{Line, Tone};
use crate::{cell_width, wrap, Look, INDENT, LABEL_WIDTH};

/// Labelled rows. A fact with nothing to say is not a row: a label over
/// an empty value tells a reader to look for something that is not
/// there.
#[derive(Default)]
pub struct Fields {
    rows: Vec<(&'static str, String)>,
}

impl Fields {
    pub fn new() -> Self {
        Fields::default()
    }

    /// These fields with `value` under `label`, when it says anything.
    pub fn push_if(mut self, label: &'static str, value: impl Into<String>) -> Self {
        let value = value.into();
        if !value.trim().is_empty() {
            self.rows.push((label, value));
        }
        self
    }
}

impl Block for Fields {
    /// Each value wrapped to what the label column leaves of the line,
    /// its later lines under its first.
    fn lines(&self, look: &Look) -> Vec<Line> {
        let under = " ".repeat(cell_width(INDENT) + LABEL_WIDTH + 1);
        let room = look.width.cells().saturating_sub(cell_width(&under));
        let mut lines = Vec::new();
        for (label, value) in &self.rows {
            for (at, part) in wrap(value, room).into_iter().enumerate() {
                lines.push(match at {
                    0 => Line::new()
                        .plain(INDENT)
                        .push(Tone::Muted, format!("{label:<LABEL_WIDTH$}"))
                        .plain(" ")
                        .plain(part),
                    _ => Line::new().plain(under.as_str()).plain(part),
                });
            }
        }
        lines
    }
}
