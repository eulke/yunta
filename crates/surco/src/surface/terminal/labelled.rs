//! Facts on labelled rows laid out at a width: the rule [`Labelled`] states, which every surface
//! draws by, a terminal at its width and a file whole.

use crate::components::{Labelled, LabelledRow};
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::width::{cell_width, truncate};

/// The cells between the widest words of a labelled block's column and its values: two, as a
/// standstill's facts and a cap's stand apart from what they say.
const VALUE_GAP: usize = 2;

impl Labelled {
    /// Its rows laid out in `cells`, a cut said in `glyphs`: those some of whose value stands
    /// after its label in their row, and every fact that holds none, the labels in one column
    /// two cells past the widest of those, each value after its label. Rows that pass `cells`
    /// are left for the surface to cut.
    pub(in crate::surface) fn laid(&self, cells: usize, glyphs: Glyphs) -> Vec<Line> {
        let fits = |row: &LabelledRow, column: usize| {
            row.value.as_ref().is_none_or(|value| {
                let room = cells.saturating_sub(column);
                if cell_width(value) <= room {
                    return true;
                }
                let cut = truncate(value, room, glyphs);
                let kept = cut.trim_end().trim_end_matches(glyphs.ellipsis());
                !kept.trim().is_empty()
            })
        };
        let mut standing: Vec<&LabelledRow> = self
            .rows
            .iter()
            .filter(|row| fits(row, label_width(row) + VALUE_GAP))
            .collect();
        loop {
            let column = column_of(&standing);
            let before = standing.len();
            standing.retain(|row| fits(row, column));
            if standing.len() == before {
                return standing.into_iter().map(|row| said(row, column)).collect();
            }
        }
    }
}

/// The cells the label of `row` takes.
fn label_width(row: &LabelledRow) -> usize {
    cell_width(&row.label.text())
}

/// The column the values of `rows` stand at: two cells past the widest of their labels.
fn column_of(rows: &[&LabelledRow]) -> usize {
    rows.iter().map(|row| label_width(row)).max().unwrap_or(0) + VALUE_GAP
}

/// `row` said with its value at `column`.
fn said(row: &LabelledRow, column: usize) -> Line {
    match &row.value {
        Some(value) => {
            let pad = " ".repeat(column.saturating_sub(label_width(row)));
            row.label
                .clone()
                .plain(pad)
                .push(Tone::Plain, value.as_str())
        }
        None => row.label.clone(),
    }
}

#[cfg(test)]
mod tests;
