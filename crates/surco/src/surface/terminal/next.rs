//! What a reader types next, each command beside what it does, or, when that
//! is too long to stand beside it, with it on the line under the command.

use crate::components::Next;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, command_lines, wrap, GAP, INDENT};

/// The title the next steps stand under, on every surface.
pub(in crate::surface) const NEXT: &str = "next";

/// The word a decision's other ways out stand under, after its options.
pub(in crate::surface) const OR: &str = "or";

impl Next {
    /// Under its title, each command beside what it does, or, when that is too
    /// long to stand beside it, with it on the line under the command.
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
/// a shell reads it as the same command. Its last line takes the gloss
/// beside it when the margin, the column, the gap and the gloss fit the
/// width; a gloss too long for that goes on the line under the command, one
/// step in and wrapped to the width, so a gloss never takes a line past it.
pub(super) fn steps(steps: &[(String, &'static str)], margin: &str, look: &Look) -> Vec<Line> {
    let room = look.width.cells().saturating_sub(cell_width(margin));
    let column = steps
        .iter()
        .map(|(command, _)| cell_width(command))
        .max()
        .unwrap_or(0)
        .min(room);
    steps
        .iter()
        .flat_map(|(command, gloss)| step(command, gloss, margin, column, look))
        .collect()
}

/// One step's lines: its command in pieces at `column` cells, the last piece
/// beside the gloss when the margin, the column, the gap and the gloss fit the
/// width, and otherwise alone, with the gloss wrapped on the line under it,
/// one step in.
fn step(command: &str, gloss: &str, margin: &str, column: usize, look: &Look) -> Vec<Line> {
    let mut pieces = command_lines(command, column);
    let last = pieces.pop().unwrap_or_default();
    let mut lines: Vec<Line> = pieces
        .into_iter()
        .map(|piece| Line::new().plain(margin).push(Tone::Strong, piece))
        .collect();
    let beside = cell_width(margin) + column + cell_width(GAP) + cell_width(gloss);
    match beside <= look.width.cells() {
        true => {
            // Padded by cells: a format width counts characters, not cells.
            let pad = " ".repeat(column.saturating_sub(cell_width(&last)));
            lines.push(
                Line::new()
                    .plain(margin)
                    .push(Tone::Strong, format!("{last}{pad}"))
                    .plain(GAP)
                    .push(Tone::Muted, gloss),
            );
        }
        false => {
            let under = format!("{margin}{INDENT}");
            let room = look.width.cells().saturating_sub(cell_width(&under));
            lines.push(Line::new().plain(margin).push(Tone::Strong, last));
            // `wrap` gives a gloss with no words one empty part, and that part has no line.
            lines.extend(
                wrap(gloss, room)
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .map(|part| Line::new().plain(under.as_str()).push(Tone::Muted, part)),
            );
        }
    }
    lines
}
