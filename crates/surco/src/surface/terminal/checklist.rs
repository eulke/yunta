//! Each check with its mark, its subject and what it found, lined up; the
//! checks that hold folded into one row when the list folds them; and
//! under a row, what its finding means and how to make it hold, each
//! beside its label.

use crate::components::{Check, Checklist, Fix, Found};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, wrap, wrap_unbroken, INDENT};

/// The label what a finding means hangs under it with, on every surface.
pub(in crate::surface) const SO: &str = "so";

/// The label how to make a finding hold hangs under it with, on every
/// surface.
pub(in crate::surface) const FIX: &str = "fix";

/// Where a list's rows put their parts: the column its subjects take,
/// the margin of what is said under a subject, and the room it wraps in.
struct Layout {
    column: usize,
    under: String,
    room: usize,
}

impl Checklist {
    /// Each check on its row: its mark, what it checked in a column as
    /// wide as the widest subject that fits in half the line, and what it
    /// found, wrapped under itself. A subject wider than that has the
    /// line to itself, and what it found starts on the next one: one long
    /// subject never pushes every other finding across the line. Under a
    /// row hang what the finding means and how to make it hold, then what
    /// says more, each line as written. A list that folds draws the checks
    /// that hold as one row naming each, a name never cut across lines, and
    /// the others under it, a line apart.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let fold = self.fold();
        let rest: Vec<&Check> = self.unfolded().collect();
        let subjects = fold.iter().map(|fold| fold.subject.as_str());
        let layout = self.layout(subjects, &rest, look);
        let mut lines = Vec::new();
        if let Some(fold) = &fold {
            let mark = Found::Holds.mark();
            let head = Line::new()
                .plain(INDENT)
                .push(Tone::of(mark), look.glyphs.mark(mark).to_string())
                .plain(" ")
                .push(Tone::Strong, fold.subject.as_str());
            let names = packed(&fold.names, look.glyphs.sep(), layout.room);
            lines.extend(beside(head, &fold.subject, names, &layout));
            if !rest.is_empty() {
                lines.push(Line::new());
            }
        }
        for check in rest {
            lines.extend(row(check, &layout, look));
        }
        lines
    }

    /// The column the subjects of the fold and of `rows`, the labels under
    /// them and the report's other subjects line up in, and what it leaves
    /// a finding.
    fn layout<'a>(
        &self,
        fold: impl Iterator<Item = &'a str>,
        rows: &[&'a Check],
        look: &Look,
    ) -> Layout {
        let lead = cell_width(INDENT) + 2;
        let half = look.width.cells() / 2;
        let labels = rows.iter().flat_map(|check| {
            [
                Some(check.subject.as_str()),
                check.so.as_ref().map(|_| SO),
                check.fix.as_ref().map(|_| FIX),
            ]
        });
        let column = fold
            .chain(labels.flatten())
            .map(cell_width)
            .chain(self.aligned.iter().map(|subject| cell_width(subject)))
            .filter(|width| *width <= half)
            .max()
            .unwrap_or(0);
        let under = " ".repeat(lead + column + 2);
        let room = look.width.cells().saturating_sub(cell_width(&under));
        Layout {
            column,
            under,
            room,
        }
    }
}

/// One check's row, then what hangs under it: what its finding means and
/// how to make it hold, beside their labels in the subjects' column, then
/// what says more.
fn row(check: &Check, layout: &Layout, look: &Look) -> Vec<Line> {
    let mark = check.found.mark();
    let head = Line::new()
        .plain(INDENT)
        .push(Tone::of(mark), look.glyphs.mark(mark).to_string())
        .plain(" ")
        .push(Tone::Strong, check.subject.as_str());
    let said = wrap(&check.said, layout.room);
    let mut lines = beside(head, &check.subject, said, layout);
    lines.extend(hangers(
        check.so.as_deref(),
        check.fix.as_ref(),
        cell_width(INDENT) * 2,
        cell_width(&layout.under),
        look,
    ));
    let hang = " ".repeat(cell_width(INDENT) * 2 + 2);
    lines.extend(
        check
            .details
            .iter()
            .flat_map(|detail| detail.lines())
            .map(|detail| Line::new().plain(hang.as_str()).plain(detail)),
    );
    lines
}

/// What a finding means and how to make it hold, each beside its label:
/// the labels `label_at` cells in, what they say `text_at` cells in,
/// wrapped to what the line leaves without cutting a word. The lines to
/// paste hang one step further in, or one step in from the labels when
/// one would run past the line there.
pub(in crate::surface) fn hangers(
    so: Option<&str>,
    fix: Option<&Fix>,
    label_at: usize,
    text_at: usize,
    look: &Look,
) -> Vec<Line> {
    let column = text_at.saturating_sub(label_at + 2);
    let layout = Layout {
        column,
        under: " ".repeat(text_at),
        room: look.width.cells().saturating_sub(text_at),
    };
    let label = |word: &str| {
        Line::new()
            .plain(" ".repeat(label_at))
            .push(Tone::Muted, word)
    };
    let mut lines = Vec::new();
    if let Some(so) = so {
        let said = wrap_unbroken(so, layout.room);
        lines.extend(beside(label(SO), SO, said, &layout));
    }
    let Some(fix) = fix else {
        return lines;
    };
    let said = wrap_unbroken(&fix.said, layout.room);
    lines.extend(beside(label(FIX), FIX, said, &layout));
    let deep = text_at + cell_width(INDENT);
    let fits = fix
        .paste
        .iter()
        .all(|line| deep + cell_width(line) <= look.width.cells());
    let margin = " ".repeat(match fits {
        true => deep,
        false => label_at + cell_width(INDENT),
    });
    lines.extend(
        fix.paste
            .iter()
            .map(|line| Line::new().plain(margin.as_str()).plain(line.as_str())),
    );
    lines
}

/// `names` on lines of `room` cells, `sep` after each but the last, a line
/// broken only between two names: a name wider than the room has a line of
/// its own.
fn packed(names: &[&str], sep: char, room: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for (at, name) in names.iter().enumerate() {
        let piece = match at + 1 < names.len() {
            true => format!("{name} {sep}"),
            false => name.to_string(),
        };
        let joined = format!("{line} {piece}");
        if line.is_empty() {
            line = piece;
        } else if cell_width(&joined) <= room {
            line = joined;
        } else {
            lines.push(std::mem::replace(&mut line, piece));
        }
    }
    lines.extend((!line.is_empty()).then_some(line));
    lines
}

/// `head`, which ends on `subject`, with `said`, already laid out, beside
/// it in the layout's column, each later line under the first; on the next
/// line when the subject is wider than the column.
fn beside(head: Line, subject: &str, said: Vec<String>, layout: &Layout) -> Vec<Line> {
    let mut said = said.into_iter();
    let mut lines = Vec::new();
    match (cell_width(subject) <= layout.column, said.next()) {
        (true, Some(first)) => {
            let gap = " ".repeat(layout.column - cell_width(subject) + 2);
            lines.push(head.plain(gap).plain(first));
        }
        (false, Some(first)) => {
            lines.push(head);
            lines.push(Line::new().plain(layout.under.as_str()).plain(first));
        }
        (_, None) => lines.push(head),
    }
    lines.extend(said.map(|part| Line::new().plain(layout.under.as_str()).plain(part)));
    lines
}
