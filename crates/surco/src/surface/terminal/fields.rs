//! Labelled facts, one per row, a long value hanging under its first line.

use crate::components::Fields;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, command_lines, wrap_unbroken, INDENT, LABEL_WIDTH};

impl Fields {
    /// Each value wrapped to what the label column leaves of the line,
    /// its later lines under its first.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let under = " ".repeat(cell_width(INDENT) + LABEL_WIDTH + 1);
        let room = look.width.cells().saturating_sub(cell_width(&under));
        let mut lines = Vec::new();
        for (label, value, tone) in &self.rows {
            let parts = match tone {
                Tone::Command => command_lines(value, room),
                _ => wrap_unbroken(value, room),
            };
            for (at, part) in parts.into_iter().enumerate() {
                lines.push(match at {
                    0 => Line::new()
                        .plain(INDENT)
                        .push(Tone::Muted, format!("{label:<LABEL_WIDTH$}"))
                        .plain(" ")
                        .push(*tone, part),
                    _ => Line::new().plain(under.as_str()).push(*tone, part),
                });
            }
        }
        lines
    }
}
