//! What a reader types next, each command beside what it does.

use crate::components::Next;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, command_lines, INDENT};

impl Next {
    /// A command wider than the line goes on under itself, cut where a
    /// shell reads it as the same command, its gloss beside its last line.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let room = look.width.cells().saturating_sub(cell_width(INDENT));
        let column = self
            .steps
            .iter()
            .map(|(command, _)| cell_width(command))
            .max()
            .unwrap_or(0)
            .min(room);
        let mut lines = Vec::new();
        for (command, gloss) in &self.steps {
            let pieces = command_lines(command, column);
            let last = pieces.len() - 1;
            for (at, piece) in pieces.into_iter().enumerate() {
                let line = Line::new().plain(INDENT);
                lines.push(match at == last {
                    true => line
                        .push(Tone::Strong, format!("{piece:<column$}"))
                        .plain("   ")
                        .push(Tone::Muted, *gloss),
                    false => line.push(Tone::Strong, piece),
                });
            }
        }
        lines
    }
}
