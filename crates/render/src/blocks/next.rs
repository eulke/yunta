//! What a reader can type next, each command with what it does.

use super::Drawn;
use crate::ink::{Line, Tone};
use crate::width::command_lines;
use crate::{cell_width, Look, INDENT};

/// The commands that move on from what a surface showed, one to a row,
/// the commands in a column so the eye runs down them and the glosses
/// beside them.
pub struct Next {
    pub steps: Vec<(String, &'static str)>,
}

impl Drawn for Next {
    /// A command wider than the line goes on under itself, cut where a
    /// shell reads it as the same command, its gloss beside its last line.
    fn lines(&self, look: &Look) -> Vec<Line> {
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
