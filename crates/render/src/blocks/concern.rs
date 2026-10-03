//! A caution a reader acts on: what is so, what it means for what they
//! decide, and what fixes it.

use super::{Drawn, Fields, Marked};
use crate::ink::Line;
use crate::{cell_width, Look, Mark};

/// What stands against a decision: the fact, what it means, and what
/// fixes it — a caution that names no way out leaves the reader with a
/// worry rather than a choice.
pub struct Concern {
    pub fact: String,
    pub so: String,
    pub fix: String,
}

impl Drawn for Concern {
    /// The fact marked and wrapped like any caution, and what it means
    /// and what fixes it under its words.
    fn lines(&self, look: &Look) -> Vec<Line> {
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
