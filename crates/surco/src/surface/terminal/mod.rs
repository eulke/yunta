//! A document on a terminal: each block laid out in columns at the
//! stream's width, with its glyph set, painted by its paint. In a pane, whose
//! edge cuts what passes it, a line past the edge goes on in the rows under it.

use crate::components::{Fold, Lead, Section};
use crate::doc::{Block, Doc, Prompt};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::surface::Surface;
use crate::width::{cell_width, INDENT};
use crate::words::Standing;

mod asked;
mod at_a_glance;
mod attention;
mod boxed;
pub use boxed::BoxSize;
mod card;
pub use card::{said_whole, Placed};
mod checklist;
pub(in crate::surface) use checklist::{FIX, SO};
mod concern;
mod decision;
mod desk;
mod diagram;
mod diffstat;
mod document;
mod evidence;
mod feed;
mod fields;
mod finding_list;
mod form;
mod headline;
mod horizon;
mod key_list;
mod labelled;
mod legend;
pub(in crate::surface) mod markdown;
mod meter;
mod next;
mod pane;
pub use pane::Paned;
mod typed;
pub(in crate::surface) use next::{NEXT, OR};
mod page;
mod problem;
mod ribbon;
pub(in crate::surface) use problem::{HELP, REFUSED};
mod runs;
mod screen;
mod scrolled;
mod survey;
mod table;
mod word_choice;
mod wordmark;

/// The cells between what stands at the two ends of a row, when both stand on it: two, so
/// the gap reads as a parting, where one would read as the space between two words.
const APART: usize = 2;

/// The fewest cells a screen's rows hold inside its margins for it to be the design's own
/// wide screen, a terminal of 100 columns: the legend's far keys and a palette's key and
/// command columns stand from there.
const WIDE_FROM: usize = 98;

/// `left` at the start of `cells` cells and `right` at its far end, with the space between
/// them as wide as the row needs. Without a `right`, `left` stands alone.
fn at_ends(left: Line, right: Option<Line>, cells: usize) -> Line {
    let Some(right) = right else {
        return left;
    };
    let gap = cells.saturating_sub(cell_width(&left.text()) + cell_width(&right.text()));
    left.plain(" ".repeat(gap)).then(right)
}

/// A terminal stream, by how its lines look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Terminal {
    pub look: Look,
    /// How a page's labelled values and its checklist's rows stand.
    values: Values,
}

/// How a page's labelled values and its checklist's rows stand at a terminal: wrapped under
/// their first column, as a page drawn whole stands; or each on its own row, cut at the
/// edge, as a page drawn close on a screen short of rows stands, every value cut to its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::surface) enum Values {
    Wrapped,
    OnTheirRow,
}

impl Terminal {
    pub fn on(look: Look) -> Self {
        Terminal {
            look,
            values: Values::Wrapped,
        }
    }

    /// This terminal with a page's labelled values and its checklist's rows each on its own
    /// row, cut at the edge.
    pub(in crate::surface) fn values_on_their_row(&self) -> Terminal {
        Terminal {
            values: Values::OnTheirRow,
            ..*self
        }
    }

    /// This terminal with its lines `width` wide, all else it draws by kept: its glyphs, its
    /// paint and its motion, and how a page's labelled values and its checklist's rows stand.
    /// Every terminal one draws inside another is built so, as a section's blocks one step in
    /// are. Not [`Terminal::on`], a terminal of a look alone, whose values stand as a page drawn
    /// whole sets them.
    pub(in crate::surface) fn at_width(&self, width: Width) -> Terminal {
        Terminal {
            look: Look { width, ..self.look },
            ..*self
        }
    }

    /// Whether this terminal's rows are the design's wide screen's, 98 cells inside the
    /// margins of 100 columns or more: there the legend's far keys stand at its far end, and
    /// a palette's box holds each act's key and command beside what it does.
    fn wide(&self) -> bool {
        self.look.width.cells() >= WIDE_FROM
    }

    /// The strokes of a run's furrow in at most `room` cells, two a node at
    /// most, where each of `nodes` stands, as a page's furrow draws them: a
    /// gap between two strokes when there are cells for one, and a cell for
    /// the nodes it covers, with the word that matters most among them, when
    /// there are more nodes than cells.
    pub fn furrow(&self, nodes: &[Standing], room: usize) -> Line {
        let cells = page::furrow_cells(nodes.len(), room);
        page::strokes(nodes, cells, page::spaced(nodes.len(), cells), &self.look)
    }

    /// `doc` laid out for this stream, unpainted, in the rhythm a page
    /// keeps: a blank line between two blocks, except the furrow, the ribbon
    /// and the gist, which hang from the headline, and a block under a heading,
    /// which hangs from it. It opens on what it says: there is nothing
    /// above the first line, and no blank line is doubled.
    pub fn lines(&self, doc: &Doc) -> Vec<Line> {
        self.in_rhythm(doc.blocks())
    }

    /// `blocks` laid out for this stream in the rhythm a page keeps, as [`Terminal::lines`]
    /// lays out a document's.
    fn in_rhythm(&self, blocks: &[Block]) -> Vec<Line> {
        let mut lines: Vec<Line> = Vec::new();
        let mut headed = false;
        let lined = runs::LinedUp::among(blocks);
        for block in blocks {
            let mut drawn = self.block_lined_up(block, lined);
            // What hangs from a heading starts on the line under it.
            if headed && drawn.first().is_some_and(|line| line.text().is_empty()) {
                drawn.remove(0);
            }
            let Some(first) = drawn.first() else {
                continue;
            };
            let hangs =
                headed || matches!(block, Block::Furrow(_) | Block::Ribbon(_) | Block::Gist(_));
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

    /// One block laid out for this stream; in a pane, each line that passes
    /// its edge goes on in the rows under it, but a command's, which its own
    /// drawing holds to the edge without parting a word.
    pub fn block(&self, block: &Block) -> Vec<Line> {
        self.block_lined_up(block, runs::LinedUp::among(std::slice::from_ref(block)))
    }

    /// One block laid out as [`Terminal::block`] lays it out, among the others of its
    /// document: a list of runs lines its columns up with every list of runs there, as
    /// `lined` says.
    fn block_lined_up(&self, block: &Block, lined: runs::LinedUp) -> Vec<Line> {
        let drawn = self.laid(block, lined);
        let width = self.look.width;
        let folds = width.cuts() && !matches!(block, Block::SameAtAShell(_));
        match folds {
            true => drawn
                .into_iter()
                .flat_map(|line| line.rows_within(width.cells()))
                .collect(),
            false => drawn,
        }
    }

    /// `block` held on a terminal `columns` wide and `rows` high while a person
    /// answers it where they are: fitted to the rows over the terminal's last, the row
    /// the cursor is left on, so drawing it never scrolls what stands above it; a list
    /// under the question its caller said above it, which stays in sight.
    pub fn held(&self, block: &Block, columns: usize, rows: usize) -> Placed {
        let said_above = usize::from(matches!(block, Block::ChoiceList(_)));
        self.held_in(block, columns, rows.saturating_sub(1 + said_above))
    }

    /// `block` held in `rows` rows `columns` wide: a prompt's block fitted to them, with
    /// where the cursor stands when it is typed into, and any other block as a document
    /// draws it. A desk lays out columns of its own across every cell it is given;
    /// everything else keeps to the measure of a line.
    fn held_in(&self, block: &Block, columns: usize, rows: usize) -> Placed {
        match block.prompt() {
            Some(Prompt::Card(card)) => card.drawn(&self.look, rows),
            Some(Prompt::Desk(desk)) => {
                let whole = Look {
                    width: Width::exact(columns),
                    ..self.look
                };
                desk.drawn(&whole, rows)
            }
            Some(Prompt::TypedAnswer(asked)) => asked.drawn(&self.look),
            Some(Prompt::ChoiceList(list)) => list.drawn(&self.look, rows),
            Some(Prompt::Form(form)) => self.form_held(form, rows),
            Some(Prompt::Survey(survey)) => {
                let held = self.survey_held(Vec::new(), survey, Vec::new(), rows);
                Placed {
                    lines: held.lines,
                    cursor: held.cursor,
                    laid: None,
                }
            }
            None => Placed {
                lines: self.block(block),
                cursor: None,
                laid: None,
            },
        }
    }

    /// One block as its own layout draws it, at the stream's width, a list of runs lined up
    /// as `lined` says. In a document a prompt takes the rows it needs: only a prompt held on
    /// a terminal fits one to the rows it has.
    fn laid(&self, block: &Block, lined: runs::LinedUp) -> Vec<Line> {
        let look = &self.look;
        match block {
            Block::Headline(block) => block.lines(look),
            Block::Furrow(block) => block.lines(look),
            Block::Ribbon(block) => block.lines(look),
            Block::Gist(block) => block.lines(look),
            Block::Fields(block) => block.lines(look, self.values),
            Block::Labelled(block) => self.rows(&block.laid(look.width.cells(), look.glyphs)),
            Block::Table(block) => block.lines(look),
            Block::Nodes(block) => block.table(look.glyphs).lines(look),
            Block::Runs(block) => block.lines(look, lined),
            Block::AtAGlance(block) => block.lines(look),
            Block::Evidence(block) => block.lines(look),
            Block::Said(block) => block.lines(look),
            Block::Paths(block) => block.lines(look),
            Block::Diffstat(block) => block.lines(look),
            Block::Decision(block) => block.lines(look),
            Block::Card(block) => block.drawn(look, usize::MAX).lines,
            Block::Desk(block) => block.drawn_whole(look),
            Block::WordChoice(block) => self.word_choice(block).into_iter().collect(),
            Block::TypedAnswer(_) | Block::ChoiceList(_) | Block::Form(_) | Block::Survey(_) => {
                self.asked_whole(block)
            }
            Block::Checklist(block) => block.lines(look, self.values),
            Block::Findings(block) => block.lines(look),
            Block::Problem(block) => block.lines(look),
            Block::Refusal(block) => block.lines(look),
            Block::Next(block) => block.lines(look),
            Block::SameAtAShell(block) => block.lines(look),
            Block::Legend(block) => block.lines(look),
            Block::KeyList(block) => block.lines(look),
            Block::Attention(block) => block.lines(look),
            Block::Noted(block) => vec![self.noted(block)],
            Block::Heading(title) => self.heading(title),
            Block::Title(title) => title.lines(look),
            Block::Section(section) | Block::Fold(Fold(section)) => self.section(section),
            Block::Prose(block) => block.lines(look),
            Block::Note(block) => block.lines(look),
            Block::Markdown(text) => self.markdown(text),
            Block::Marked(block) => block.lines(look),
            Block::Concern(block) => block.lines(look),
            Block::Diagram(block) => block.lines(look),
            Block::Code(block) => block.lines(look),
            Block::Lines(lines) => lines.clone(),
            Block::Rows(rows) => self.rows(rows),
            Block::Feed(block) => block.lines(look),
            Block::Meter(block) => vec![self.meter(block)],
            Block::Wordmark(_) => self.wordmark(),
            Block::Horizon(block) => self.horizon_in_a_document(block),
        }
    }
}

impl Terminal {
    /// A line asked, a list, a form or a survey, whole, as a document draws it: each takes
    /// the rows it needs. Nothing for any other block.
    fn asked_whole(&self, block: &Block) -> Vec<Line> {
        let look = &self.look;
        match block.prompt() {
            Some(Prompt::TypedAnswer(asked)) => asked.drawn(look).lines,
            Some(Prompt::ChoiceList(list)) => list.drawn(look, usize::MAX).lines,
            Some(Prompt::Form(form)) => self.form(form),
            Some(Prompt::Survey(survey)) => self.survey(survey),
            Some(Prompt::Card(_) | Prompt::Desk(_)) | None => Vec::new(),
        }
    }

    /// Rows a pointer moves along, each one row: cut at the stream's width with the ellipsis,
    /// never folded onto the row under it.
    fn rows(&self, rows: &[Line]) -> Vec<Line> {
        rows.iter()
            .map(|row| row.cut(self.look.width.cells(), self.look.glyphs))
            .collect()
    }

    /// The title of what follows it, inside a document: a blank line, then its words muted,
    /// going on in the lines under it where they pass the line.
    fn heading(&self, title: &str) -> Vec<Line> {
        std::iter::once(Line::new())
            .chain(
                crate::width::wrap_unbroken(title, self.look.width.cells())
                    .into_iter()
                    .map(|part| Line::new().push(Tone::Muted, part)),
            )
            .collect()
    }

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
        let inner = self.one_step_under();
        let mut lines = self.section_title(section);
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

    /// This stream one step in: what is left of the line under [`INDENT`], where a section's
    /// blocks are laid out.
    fn one_step_under(&self) -> Terminal {
        let width = self.look.width;
        self.at_width(width.within(width.cells().saturating_sub(cell_width(INDENT))))
    }

    /// A section's title, one step in, after a blank line: led by its lead, and going on
    /// under itself, past its lead, where it is wider than the line.
    fn section_title(&self, section: &Section) -> Vec<Line> {
        let inner = self.one_step_under();
        let lead = match section.lead {
            Some(Lead::Mark(mark)) => Line::new()
                .push(Tone::of(mark), self.look.glyphs.mark(mark).to_string())
                .plain(" "),
            Some(Lead::Ordinal(at)) => Line::new().push(Tone::Strong, format!("{at}  ")),
            Some(Lead::Pointer { on: true }) => Line::new()
                .push(Tone::Strong, self.look.glyphs.pointer().to_string())
                .plain(" "),
            Some(Lead::Pointer { on: false }) => Line::new().plain("  "),
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
