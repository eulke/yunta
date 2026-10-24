//! What a reader types next, each command beside what it does.

use crate::components::Next;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, command_lines, INDENT};

/// The title the next steps stand under, on every surface.
pub(in crate::surface) const NEXT: &str = "next";

/// The word a decision's other ways out stand under, after its options.
pub(in crate::surface) const OR: &str = "or";

impl Next {
    /// Under its title, each command beside what it does.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        if self.steps.is_empty() {
            return Vec::new();
        }
        let mut lines = vec![Line::new().push(Tone::Muted, NEXT)];
        lines.extend(steps(&self.steps, INDENT, look));
        lines
    }
}

/// Each command `margin` in, beside what it does, the commands in one
/// column. A command wider than the line goes on under itself, cut where
/// a shell reads it as the same command, its gloss beside its last line.
pub(super) fn steps(steps: &[(String, &'static str)], margin: &str, look: &Look) -> Vec<Line> {
    let room = look.width.cells().saturating_sub(cell_width(margin));
    let column = steps
        .iter()
        .map(|(command, _)| cell_width(command))
        .max()
        .unwrap_or(0)
        .min(room);
    let mut lines = Vec::new();
    for (command, gloss) in steps {
        let pieces = command_lines(command, column);
        let last = pieces.len() - 1;
        for (at, piece) in pieces.into_iter().enumerate() {
            let line = Line::new().plain(margin);
            lines.push(match at == last {
                true => line
                    .push(Tone::Strong, format!("{piece:<column$}"))
                    .plain(crate::width::GAP)
                    .push(Tone::Muted, *gloss),
                false => line.push(Tone::Strong, piece),
            });
        }
    }
    lines
}
