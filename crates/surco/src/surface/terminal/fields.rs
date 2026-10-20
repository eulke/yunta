//! Labelled facts, one per row, a long value hanging under its first line.

use crate::components::Fields;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, command_lines, wrap_unbroken, INDENT, LABEL_WIDTH};

impl Fields {
    /// Each value wrapped to what the label column leaves of the line,
    /// its later lines under its first; a label repeated from the row
    /// above is not said again, and a marked value hangs under its words.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let column = " ".repeat(cell_width(INDENT) + LABEL_WIDTH + 1);
        let mut lines = Vec::new();
        let mut above = None;
        for field in &self.rows {
            let mark = field
                .mark
                .map(|mark| (Tone::of(mark), format!("{} ", look.glyphs.mark(mark))));
            let under = format!(
                "{column}{}",
                " ".repeat(mark.as_ref().map_or(0, |(_, glyph)| cell_width(glyph)))
            );
            let room = look.width.cells().saturating_sub(cell_width(&under));
            let parts = match field.tone {
                Tone::Command => command_lines(&field.value, room),
                _ => wrap_unbroken(&field.value, room),
            };
            let label = match above == Some(field.label) {
                true => "",
                false => field.label,
            };
            above = Some(field.label);
            for (at, part) in parts.into_iter().enumerate() {
                lines.push(match (at, &mark) {
                    (0, mark) => {
                        let line = Line::new()
                            .plain(INDENT)
                            .push(Tone::Muted, format!("{label:<LABEL_WIDTH$}"))
                            .plain(" ");
                        match mark {
                            Some((tone, glyph)) => line.push(*tone, glyph.as_str()),
                            None => line,
                        }
                        .push(field.tone, part)
                    }
                    _ => Line::new().plain(under.as_str()).push(field.tone, part),
                });
            }
        }
        lines
    }
}
