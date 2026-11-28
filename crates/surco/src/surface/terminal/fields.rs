//! Labelled facts, one per row, a long value hanging under its first line. The labels stand
//! in one column ([`LabelColumn`]), as a run's at a glance stand.

use super::Values;
use crate::components::{Fields, Reads};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, command_lines, wrap_unbroken, INDENT, LABEL_WIDTH};

/// The column labels stand in, each value after it, as a page's facts and a run's at a glance
/// stand: one step in, [`LABEL_WIDTH`] cells, or as many as the widest label where one is
/// wider, so every value stands in one column; then the cell after it. A value's later rows
/// stand under its first cell. Not [`Labelled`](crate::components::Labelled)'s column, two
/// cells past the widest of the rows that stand, whose values are figures cut whole.
pub(super) struct LabelColumn {
    cells: usize,
}

impl LabelColumn {
    /// The column `labels` stand in.
    pub(super) fn of<'a>(labels: impl IntoIterator<Item = &'a str>) -> LabelColumn {
        let widest = labels.into_iter().map(cell_width).max().unwrap_or(0);
        LabelColumn {
            cells: widest.max(LABEL_WIDTH),
        }
    }

    /// A value's first row up to the value: one step in, `label` muted across the column, and
    /// the cell after it.
    pub(super) fn opening(&self, label: &str) -> Line {
        let pad = " ".repeat(self.cells.saturating_sub(cell_width(label)));
        Line::new()
            .plain(INDENT)
            .push(Tone::Muted, format!("{label}{pad}"))
            .plain(" ")
    }

    /// The blank cells a value's later rows stand under: one step in, the column, and the
    /// cell after it.
    pub(super) fn under(&self) -> String {
        " ".repeat(cell_width(INDENT) + self.cells + 1)
    }
}

impl Fields {
    /// Each value wrapped to what the label column leaves of the line,
    /// its later lines under its first; a label repeated from the row
    /// above is not said again, and a marked value hangs under its words.
    /// Text read as written is drawn as words are: a terminal reads no
    /// sign of it as markup. Where `values` stand on their row, each value
    /// stands whole after its label, the row cut at the line's end.
    pub(in crate::surface) fn lines(&self, look: &Look, values: Values) -> Vec<Line> {
        let labels = LabelColumn::of(self.rows.iter().map(|field| field.label));
        let column = labels.under();
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
            let (tone, parts) = parts((field.reads, &field.value), room, values);
            let label = match above == Some(field.label) {
                true => "",
                false => field.label,
            };
            above = Some(field.label);
            for (at, part) in parts.into_iter().enumerate() {
                lines.push(match (at, &mark) {
                    (0, mark) => {
                        let line = labels.opening(label);
                        match mark {
                            Some((tone, glyph)) => line.push(*tone, glyph.as_str()),
                            None => line,
                        }
                        .push(tone, part)
                    }
                    _ => Line::new().plain(under.as_str()).push(tone, part),
                });
            }
        }
        match values {
            Values::Wrapped => lines,
            Values::OnTheirRow => lines
                .into_iter()
                .map(|line| line.cut(look.width.cells(), look.glyphs))
                .collect(),
        }
    }
}

/// A value that `reads` as it says, in the tone it reads in and the parts it stands in:
/// wrapped to `room`, a command at a space no quote holds, where `values` are wrapped; whole,
/// where they stand on their row.
fn parts((reads, value): (Reads, &str), room: usize, values: Values) -> (Tone, Vec<String>) {
    match (reads, values) {
        (Reads::Command, Values::Wrapped) => (Tone::Command, command_lines(value, room)),
        (Reads::Command, Values::OnTheirRow) => (Tone::Command, vec![value.to_string()]),
        (Reads::Words | Reads::Literal, Values::Wrapped) => {
            (Tone::Plain, wrap_unbroken(value, room))
        }
        (Reads::Words | Reads::Literal, Values::OnTheirRow) => {
            (Tone::Plain, vec![value.to_string()])
        }
    }
}
