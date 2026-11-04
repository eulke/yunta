//! A caution, what it means and what fixes it.

use crate::components::{Concern, Marked};
use crate::glyphs::Mark;
use crate::line::Line;
use crate::look::Look;
use crate::width::{cell_width, INDENT, LABEL_WIDTH};

impl Concern {
    /// The fact marked and wrapped like any caution, and what it means
    /// and what fixes it under its words.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let mut lines = Marked {
            mark: Mark::Caution,
            items: vec![self.fact.clone()],
        }
        .lines(look);
        // What it means and what fixes it hang under its words, in the
        // columns a page's fields take.
        let step = cell_width(&look.glyphs.mark(Mark::Caution).to_string()) + 1;
        let label_at = step + cell_width(INDENT);
        lines.extend(super::checklist::hangers(
            Some(self.so.as_str()),
            Some(&self.fix),
            label_at,
            label_at + LABEL_WIDTH + 1,
            look,
        ));
        lines
    }
}
