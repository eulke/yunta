//! A document on a terminal: each block laid out in columns at the
//! stream's width, with its glyph set, painted by its ink.

use super::Surface;
use crate::blocks::Drawn;
use crate::doc::{Block, Doc};
use crate::ink::{Line, Tone};
use crate::{Look, INDENT};

/// A terminal stream, by how its lines look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Terminal {
    pub look: Look,
}

impl Terminal {
    pub fn on(look: Look) -> Self {
        Terminal { look }
    }

    /// `doc` laid out for this stream, unpainted.
    pub fn lines(&self, doc: &Doc<'_>) -> Vec<Line> {
        doc.blocks()
            .iter()
            .flat_map(|block| self.block(block))
            .collect()
    }

    fn block(&self, block: &Block<'_>) -> Vec<Line> {
        let look = &self.look;
        match block {
            Block::Headline(block) => block.lines(look),
            Block::Fields(block) => block.lines(look),
            Block::NodeTable(block) => block.lines(look),
            Block::Evidence(block) => block.lines(look),
            Block::Failure(block) => block.lines(look),
            Block::Decision(block) => block.lines(look),
            Block::Checklist(block) => block.lines(look),
            Block::Next(block) => block.lines(look),
            Block::Heading(title) => vec![
                Line::new(),
                Line::new().plain(INDENT).push(Tone::Strong, title.as_str()),
            ],
            Block::Lines(lines) => lines.clone(),
        }
    }
}

impl Surface for Terminal {
    /// Every line painted and ended.
    type Output = String;

    fn draw(&self, doc: &Doc<'_>) -> String {
        self.lines(doc)
            .iter()
            .map(|line| format!("{}\n", self.look.ink.paint(line)))
            .collect()
    }
}
