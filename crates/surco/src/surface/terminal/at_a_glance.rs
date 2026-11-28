//! A run at a glance, at a terminal: its facts one step in, each label muted in the column a
//! page's facts stand in (`fields`' `LabelColumn`), and its value after it, the values in one
//! column. A value that passes its row wraps between its words under its own first cell,
//! never under its label and never inside a figure; a word wider than its row is cut at the
//! row's end by the one cut rule. Then, a blank row apart, the acts: their heading muted at
//! the glance's first cell, then a key a row, the key strong in the column a list of keys
//! takes (seven cells, or the widest key and two), what it does after it and cut at the row's
//! end, the key whole.

use super::fields::LabelColumn;
use super::key_list::key_column;
use crate::components::AtAGlance;
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::width::{cell_width, wrap_unbroken};

impl AtAGlance {
    /// The facts that say something, then a blank row and the acts where both stand.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let facts = self.fact_lines(look);
        let acts = self.act_lines(look);
        let apart = (!facts.is_empty() && !acts.is_empty()).then(Line::new);
        facts.into_iter().chain(apart).chain(acts).collect()
    }

    /// Each fact that says something: its label in the column, its value after it and its
    /// later rows under its first cell.
    fn fact_lines(&self, look: &Look) -> Vec<Line> {
        let cells = look.width.cells();
        let column = LabelColumn::of(self.said().map(|(label, _)| *label));
        let under = column.under();
        let room = cells.saturating_sub(cell_width(&under));
        let mut lines = Vec::new();
        for (label, value) in self.said() {
            for (at, part) in wrap_unbroken(value, room).into_iter().enumerate() {
                let lead = match at {
                    0 => column.opening(label),
                    _ => Line::new().plain(under.as_str()),
                };
                lines.push(lead.plain(part).cut(cells, look.glyphs));
            }
        }
        lines
    }

    /// The acts' heading, then a key a row; nothing where the glance offers no act.
    fn act_lines(&self, look: &Look) -> Vec<Line> {
        let Some((heading, acts)) = self.acts.as_ref().filter(|(_, acts)| !acts.is_empty()) else {
            return Vec::new();
        };
        let cells = look.width.cells();
        let spelled: Vec<&str> = acts
            .iter()
            .map(|act| act.keys.spelled(look.glyphs))
            .collect();
        let column = key_column(&spelled);
        let heading = Line::new()
            .push(Tone::Muted, heading.as_str())
            .cut(cells, look.glyphs);
        let rows = spelled.iter().zip(acts).map(|(key, act)| {
            let does = Line::new()
                .plain(act.does.as_str())
                .cut(cells.saturating_sub(column), look.glyphs);
            let pad = " ".repeat(column - cell_width(key));
            Line::new()
                .push(Tone::Strong, format!("{key}{pad}"))
                .then(does)
        });
        std::iter::once(heading).chain(rows).collect()
    }
}

#[cfg(test)]
mod tests;
