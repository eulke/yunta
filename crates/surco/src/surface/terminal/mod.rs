//! A document on a terminal: each block laid out in columns at the
//! stream's width, with its glyph set, painted by its paint.

use crate::components::{Lead, Section};
use crate::doc::{Block, Doc};
use crate::line::{Line, Tone};
use crate::look::Look;
use crate::surface::Surface;
use crate::width::{cell_width, INDENT};
use crate::words::Standing;

mod card;
pub use card::{narrowed, said_whole, Placed};
pub use desk::LaidDesk;
mod checklist;
pub(in crate::surface) use checklist::{FIX, SO};
mod concern;
mod decision;
mod desk;
mod diagram;
mod diffstat;
mod document;
mod evidence;
mod fields;
mod headline;
pub(in crate::surface) mod markdown;
mod next;
pub(in crate::surface) use next::{NEXT, OR};
mod page;
mod problem;
pub(in crate::surface) use problem::{HELP, REFUSED};
mod runs;
mod table;

/// A terminal stream, by how its lines look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Terminal {
    pub look: Look,
}

impl Terminal {
    pub fn on(look: Look) -> Self {
        Terminal { look }
    }

    /// The strokes of a run's furrow in at most `room` cells, two a node at
    /// most, where each of `nodes` stands, as a page's furrow draws them: a
    /// gap between two strokes when there are cells for one, and a cell for
    /// the nodes it covers, with the word that matters most among them, when
    /// there are more nodes than cells.
    pub fn furrow(&self, nodes: &[Standing], room: usize) -> Line {
        let cells = room.min(nodes.len() * page::CELLS_A_NODE);
        page::strokes(nodes, cells, page::spaced(nodes.len(), cells), &self.look)
    }

    /// `doc` laid out for this stream, unpainted, in the rhythm a page
    /// keeps: a blank line between two blocks, except the furrow and the
    /// gist, which hang from the headline, and a block under a heading,
    /// which hangs from it. It opens on what it says: there is nothing
    /// above the first line, and no blank line is doubled.
    pub fn lines(&self, doc: &Doc) -> Vec<Line> {
        let mut lines: Vec<Line> = Vec::new();
        let mut headed = false;
        for block in doc.blocks() {
            let mut drawn = self.block(block);
            // What hangs from a heading starts on the line under it.
            if headed && drawn.first().is_some_and(|line| line.text().is_empty()) {
                drawn.remove(0);
            }
            let Some(first) = drawn.first() else {
                continue;
            };
            let hangs = headed || matches!(block, Block::Furrow(_) | Block::Gist(_));
            let spaced =
                first.text().is_empty() || lines.last().is_none_or(|line| line.text().is_empty());
            if !hangs && !spaced {
                lines.push(Line::new());
            }
            lines.extend(drawn);
            headed = matches!(block, Block::Heading(_));
        }
        let opens = lines
            .iter()
            .position(|line| !line.text().is_empty())
            .unwrap_or(lines.len());
        lines.drain(..opens);
        lines
    }

    /// Each of `blocks` laid out for this stream, in order.
    pub fn blocks(&self, blocks: &[Block]) -> Vec<Line> {
        blocks.iter().flat_map(|block| self.block(block)).collect()
    }

    /// One block laid out for this stream.
    pub fn block(&self, block: &Block) -> Vec<Line> {
        let look = &self.look;
        match block {
            Block::Headline(block) => block.lines(look),
            Block::Furrow(block) => block.lines(look),
            Block::Gist(block) => block.lines(look),
            Block::Fields(block) => block.lines(look),
            Block::Table(block) => block.lines(look),
            Block::Nodes(block) => block.table(look.glyphs).lines(look),
            Block::Runs(block) => block.lines(look),
            Block::Evidence(block) => block.lines(look),
            Block::Said(block) => block.lines(look),
            Block::Paths(block) => block.lines(look),
            Block::Diffstat(block) => block.lines(look),
            Block::Decision(block) => block.lines(look),
            Block::Checklist(block) => block.lines(look),
            Block::Problem(block) => block.lines(look),
            Block::Refusal(block) => block.lines(look),
            Block::Next(block) => block.lines(look),
            Block::Heading(title) => std::iter::once(Line::new())
                .chain(
                    crate::width::wrap_unbroken(title, look.width.cells())
                        .into_iter()
                        .map(|part| Line::new().push(Tone::Muted, part)),
                )
                .collect(),
            Block::Title(title) => title.lines(look),
            Block::Section(section) => self.section(section),
            Block::Fold(fold) => self.section(&fold.0),
            Block::Prose(block) => block.lines(look),
            Block::Note(block) => block.lines(look),
            Block::Markdown(text) => self.markdown(text),
            Block::Marked(block) => block.lines(look),
            Block::Concern(block) => block.lines(look),
            Block::Diagram(block) => block.lines(look),
            Block::Code(block) => block.lines(look),
            Block::Lines(lines) => lines.clone(),
        }
    }
}

impl Terminal {
    /// Markdown an author wrote: its text as prose and code, and each
    /// diagram in it drawn as one.
    fn markdown(&self, text: &str) -> Vec<Line> {
        use crate::diagram::split::{parts, Part};
        let look = &self.look;
        parts(text)
            .into_iter()
            .enumerate()
            .flat_map(|(at, part)| {
                let mut lines = match at {
                    0 => Vec::new(),
                    _ => vec![Line::new()],
                };
                lines.extend(match part {
                    Part::Text(text) => markdown::markdown(&text, INDENT, look),
                    Part::Diagram(diagram) => diagram.lines(look),
                });
                lines
            })
            .collect()
    }

    /// A section: its title, and its blocks one step under it, laid out
    /// for what is left of the line there.
    fn section(&self, section: &Section) -> Vec<Line> {
        let inner = Terminal::on(Look {
            width: self
                .look
                .width
                .within(self.look.width.cells().saturating_sub(cell_width(INDENT))),
            ..self.look
        });
        // A title wider than the line goes on under itself, past its lead.
        let lead = match section.lead {
            Some(Lead::Mark(mark)) => Line::new()
                .push(Tone::of(mark), self.look.glyphs.mark(mark).to_string())
                .plain(" "),
            Some(Lead::Ordinal(at)) => Line::new().push(Tone::Strong, format!("{at}  ")),
            None => Line::new(),
        };
        let hang = " ".repeat(cell_width(&lead.text()));
        let room = inner.look.width.cells().saturating_sub(cell_width(&hang));
        let mut lines = vec![Line::new()];
        for (at, part) in section.title.wrap(room).into_iter().enumerate() {
            let part = match at {
                0 => lead.clone().then(part),
                _ => part.under(&hang),
            };
            lines.push(part.under(INDENT));
        }
        let mut after_section = false;
        for block in &section.blocks {
            let drawn = inner.block(block);
            let Some(first) = drawn.first() else {
                continue;
            };
            // A section or a fold inside this one ends a block of its own:
            // what follows it stands apart from it.
            if after_section && !first.text().is_empty() {
                lines.push(Line::new());
            }
            // A blank line stays blank: a margin on it is trailing space a
            // reader cannot see and a diff can.
            lines.extend(drawn.into_iter().map(|line| match line.text().is_empty() {
                true => line,
                false => line.under(INDENT),
            }));
            after_section = matches!(block, Block::Section(_) | Block::Fold(_));
        }
        lines
    }
}

impl Surface for Terminal {
    /// Every line painted and ended.
    type Output = String;

    fn draw(&self, doc: &Doc) -> String {
        self.lines(doc)
            .iter()
            .map(|line| format!("{}\n", self.look.paint.line(line)))
            .collect()
    }
}

#[cfg(test)]
mod tests;
