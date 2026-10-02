//! A document on a terminal: each block laid out in columns at the
//! stream's width, with its glyph set, painted by its ink.

use super::Surface;
use crate::blocks::Drawn;
use crate::blocks::Section;
use crate::doc::{Block, Doc};
use crate::ink::{Line, Tone};
use crate::{cell_width, Look, INDENT};

/// A terminal stream, by how its lines look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Terminal {
    pub look: Look,
}

impl Terminal {
    pub fn on(look: Look) -> Self {
        Terminal { look }
    }

    /// `doc` laid out for this stream, unpainted. It opens on what it
    /// says: the blank line a section keeps above itself is room between
    /// two things, and there is nothing above the first.
    pub fn lines(&self, doc: &Doc<'_>) -> Vec<Line> {
        doc.blocks()
            .iter()
            .flat_map(|block| self.block(block))
            .skip_while(|line| line.text().is_empty())
            .collect()
    }

    /// One block laid out for this stream.
    pub fn block(&self, block: &Block<'_>) -> Vec<Line> {
        let look = &self.look;
        match block {
            Block::Headline(block) => block.lines(look),
            Block::Fields(block) => block.lines(look),
            Block::Table(block) => block.lines(look),
            Block::Evidence(block) => block.lines(look),
            Block::Failure(block) => block.lines(look),
            Block::Decision(block) => block.lines(look),
            Block::Checklist(block) => block.lines(look),
            Block::Next(block) => block.lines(look),
            Block::Heading(title) => vec![
                Line::new(),
                Line::new().plain(INDENT).push(Tone::Strong, title.as_str()),
            ],
            Block::Title(title) => vec![title.clone()],
            Block::Section(section) => self.section(section),
            Block::Prose(block) => block.lines(look),
            Block::Markdown(text) => crate::markdown::markdown(text, INDENT, look.width.cells())
                .into_iter()
                .map(|line| Line::new().plain(line))
                .collect(),
            Block::Marked(block) => block.lines(look),
            Block::Concern(block) => block.lines(look),
            Block::Code(block) => block.lines(look),
            Block::Lines(lines) => lines.clone(),
        }
    }
}

impl Terminal {
    /// A section: its title, and its blocks one step under it, laid out
    /// for what is left of the line there.
    fn section(&self, section: &Section<'_>) -> Vec<Line> {
        let inner = Terminal::on(Look {
            width: self
                .look
                .width
                .within(self.look.width.cells().saturating_sub(cell_width(INDENT))),
            ..self.look
        });
        let title = match section.mark {
            Some(mark) => Line::new()
                .push(Tone::of(mark), self.look.glyphs.mark(mark).to_string())
                .plain(" ")
                .then(section.title.clone()),
            None => section.title.clone(),
        };
        let mut lines = vec![Line::new(), title.under(INDENT)];
        lines.extend(
            section
                .blocks
                .iter()
                .flat_map(|block| inner.block(block))
                // A blank line stays blank: a margin on it is trailing
                // space a reader cannot see and a diff can.
                .map(|line| match line.text().is_empty() {
                    true => line,
                    false => line.under(INDENT),
                }),
        );
        lines
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
