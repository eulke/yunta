//! What a set of checks found, one row each: the mark that says whether
//! it holds, what was checked, and what was found.

use super::Drawn;
use crate::ink::{Line, Tone};
use crate::{cell_width, wrap, Look, Mark, INDENT};

/// What one check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// It holds.
    Holds,
    /// Worth a reader's attention; nothing stops because of it.
    Caution,
    /// A run that reaches it stops there.
    Problem,
}

impl Found {
    /// The mark a finding of this kind is drawn with.
    pub fn mark(self) -> Mark {
        match self {
            Found::Holds => Mark::Done,
            Found::Caution => Mark::Caution,
            Found::Problem => Mark::Failed,
        }
    }
}

/// One check: what it found, what it checked, and what it found there.
pub struct Check {
    pub found: Found,
    pub subject: String,
    pub said: String,
}

/// The checks of one report, in the order they ran.
#[derive(Default)]
pub struct Checklist {
    checks: Vec<Check>,
}

impl Checklist {
    /// This list with one more check on it.
    pub fn push(&mut self, found: Found, subject: impl Into<String>, said: impl Into<String>) {
        self.checks.push(Check {
            found,
            subject: subject.into(),
            said: said.into(),
        });
    }

    /// This list with `check` on it.
    pub fn push_check(&mut self, check: Check) {
        self.checks.push(check);
    }

    /// This list with `checks` on it, in their order.
    pub fn extend(&mut self, checks: impl IntoIterator<Item = Check>) {
        self.checks.extend(checks);
    }

    /// Whether nothing on the list stops a run.
    /// Each check, in the order it was made.
    pub fn checks(&self) -> impl Iterator<Item = &Check> {
        self.checks.iter()
    }

    pub fn holds(&self) -> bool {
        self.checks
            .iter()
            .all(|check| check.found != Found::Problem)
    }
}

impl Drawn for Checklist {
    /// Each check on its row: its mark, what it checked in a column as
    /// wide as the widest subject that fits in half the line, and what it
    /// found, wrapped under itself. A subject wider than that has the
    /// line to itself, and what it found starts on the next one — one
    /// long subject never pushes every other finding across the line.
    fn lines(&self, look: &Look) -> Vec<Line> {
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
