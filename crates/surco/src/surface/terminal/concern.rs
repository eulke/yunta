//! A caution, what it means and what fixes it.

use crate::components::{Concern, Fields, Marked};
use crate::glyphs::Mark;
use crate::line::Line;
use crate::look::Look;
use crate::width::cell_width;

impl Concern {
    /// The fact marked and wrapped like any caution, and what it means
    /// and what fixes it under its words.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        let mut lines = Marked {
            mark: Mark::Caution,
            items: vec![self.fact.clone()],
        }
        .lines(look);
        let step = " ".repeat(cell_width(&look.glyphs.mark(Mark::Caution).to_string()) + 1);
        let inner = Look {
            width: look
                .width
                .within(look.width.cells().saturating_sub(step.len())),
            ..*look
        };
        lines.extend(
            Fields::new()
                .push_if("so", self.so.as_str())
                .push_if("fix", self.fix.as_str())
                .lines(&inner)
                .into_iter()
                .map(|line| line.under(&step)),
        );
        lines
    }
}
