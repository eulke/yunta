//! Each check with its mark, its subject and what it found, lined up.

use crate::components::Checklist;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, wrap, INDENT};

impl Checklist {
    /// Each check on its row: its mark, what it checked in a column as
    /// wide as the widest subject that fits in half the line, and what it
    /// found, wrapped under itself. A subject wider than that has the
    /// line to itself, and what it found starts on the next one — one
    /// long subject never pushes every other finding across the line.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let lead = cell_width(INDENT) + 2;
        let half = look.width.cells() / 2;
        let column = self
            .checks
            .iter()
            .map(|check| cell_width(&check.subject))
            .filter(|width| *width <= half)
            .max()
            .unwrap_or(0);
        let under = " ".repeat(lead + column + 2);
        let room = look.width.cells().saturating_sub(cell_width(&under));
        let mut lines = Vec::new();
        for check in &self.checks {
            let mark = check.found.mark();
            let head = Line::new()
                .plain(INDENT)
                .push(Tone::of(mark), look.glyphs.mark(mark).to_string())
                .plain(" ")
                .push(Tone::Strong, check.subject.as_str());
            let mut said = wrap(&check.said, room).into_iter();
            let fits = cell_width(&check.subject) <= column;
            match (fits, said.next()) {
                (true, Some(first)) => {
                    let gap = " ".repeat(column - cell_width(&check.subject) + 2);
                    lines.push(head.plain(gap).plain(first));
                }
                (false, Some(first)) => {
                    lines.push(head);
                    lines.push(Line::new().plain(under.as_str()).plain(first));
                }
                (_, None) => lines.push(head),
            }
            lines.extend(said.map(|part| Line::new().plain(under.as_str()).plain(part)));
        }
        lines
    }
}
