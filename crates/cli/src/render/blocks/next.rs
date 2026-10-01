//! What a reader can type next, each command with what it does.

use super::Block;
use crate::render::ink::{Line, Tone};
use crate::render::{cell_width, Look, INDENT};

/// The commands that move on from what a surface showed, one to a row,
/// the commands in a column so the eye runs down them and the glosses
/// beside them.
pub(crate) struct Next {
    pub(crate) steps: Vec<(String, &'static str)>,
}

impl Block for Next {
    fn lines(&self, _look: &Look) -> Vec<Line> {
        let column = self
            .steps
            .iter()
            .map(|(command, _)| cell_width(command))
            .max()
            .unwrap_or(0);
        self.steps
            .iter()
            .map(|(command, gloss)| {
                Line::new()
                    .plain(INDENT)
                    .push(Tone::Strong, format!("{command:<column$}"))
                    .plain("   ")
                    .push(Tone::Muted, *gloss)
            })
            .collect()
    }
}
