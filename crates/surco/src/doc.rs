//! What a surface says, apart from how any medium draws it.
//!
//! A surface builds a [`Doc`] from blocks — what happened, what needs a
//! person, the evidence, the detail, what to type next — and a
//! [`Surface`](crate::surface::Surface) draws it: a terminal lays it out
//! in columns at a width, painted for its stream, and a file writes it as
//! Markdown. A block holds what is said and the role each part plays,
//! never a width, an escape or a glyph, so a medium added later — a page
//! in a browser, a window of an application — draws the documents every
//! surface already builds, and changes none of them.

use crate::components::{
    Checklist, Code, Concern, Decision, Evidence, Fields, Furrow, Gist, Headline, Marked, Next,
    NodeTable, Note, Paths, Problem, Prose, Refusal, Runs, Said, Section, Table,
};
use crate::line::Line;

/// How much of a page its reader asked for. A page opens at a glance —
/// where the thing stands, what it asks, what failed — and what the
/// glance leaves out is one step away: the whole story, then every trace
/// kept. No fact is only at a depth nothing reaches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Depth {
    /// What a reader needs to act on: the page a command opens on.
    #[default]
    Glance,
    /// The whole story: every part with what it said, took and spent.
    Full,
    /// Every trace kept: whole ids, the paths things live at, whole
    /// outputs.
    Dig,
}

impl Depth {
    /// The depth of a reader who asked for the whole story (`full`) or for
    /// every trace (`dig`): digging tells the whole story too.
    pub fn of(full: bool, dig: bool) -> Self {
        match (full, dig) {
            (_, true) => Depth::Dig,
            (true, false) => Depth::Full,
            (false, false) => Depth::Glance,
        }
    }

    /// Whether a page at this depth says what a page at `depth` says.
    pub fn reaches(self, depth: Depth) -> bool {
        self >= depth
    }
}

/// One part of a document.
pub enum Block {
    Headline(Headline),
    /// Where each node of the run stands, under the headline.
    Furrow(Furrow),
    /// The page's gist, under the headline.
    Gist(Gist),
    Fields(Fields),
    /// Rows under named columns.
    Table(Table),
    /// A run's nodes, one to a row: a table each surface builds in its own
    /// glyphs, since a row's branch and return are drawn in them.
    Nodes(NodeTable),
    /// A field of runs, a row to each.
    Runs(Runs),
    Evidence(Evidence),
    /// Lines a message says whole, as its author broke them.
    Said(Said),
    /// A heading, and each path it names.
    Paths(Paths),
    Decision(Decision),
    Checklist(Checklist),
    /// A problem found in a text, pointed at in its line.
    Problem(Problem),
    /// What a command would not do.
    Refusal(Refusal),
    Next(Next),
    /// What a whole document is, before anything it says: its name and
    /// the facts beside it.
    Title(crate::components::Title),
    /// The title of what follows it, inside a document.
    Heading(String),
    Section(Section),
    /// A section a reader opens when they want what it holds.
    Fold(crate::components::Fold),
    Prose(Prose),
    /// A remark beside what the page says.
    Note(Note),
    /// Text already written as Markdown — a planner's description, a
    /// reviewer's detail — which a file keeps as written and a terminal
    /// reads as prose and code.
    Markdown(String),
    Marked(Marked),
    /// A caution with what it means and what fixes it.
    Concern(Concern),
    /// A diagram an author wrote, which each surface draws its own way.
    Diagram(crate::diagram::Diagram),
    Code(Code),
    /// Lines a surface composed itself, each span carrying its role.
    Lines(Vec<Line>),
}

macro_rules! block_from {
    ($($variant:ident($kind:ty)),* $(,)?) => {
        $(impl From<$kind> for Block {
            fn from(block: $kind) -> Self {
                Block::$variant(block)
            }
        })*
    };
}

block_from!(
    Headline(Headline),
    Furrow(Furrow),
    Gist(Gist),
    Fields(Fields),
    Table(Table),
    Nodes(NodeTable),
    Runs(Runs),
    Evidence(Evidence),
    Said(Said),
    Paths(Paths),
    Decision(Decision),
    Checklist(Checklist),
    Problem(Problem),
    Refusal(Refusal),
    Next(Next),
    Title(crate::components::Title),
    Section(Section),
    Fold(crate::components::Fold),
    Prose(Prose),
    Note(Note),
    Marked(Marked),
    Concern(Concern),
    Diagram(crate::diagram::Diagram),
    Code(Code),
    Lines(Vec<Line>),
);

/// What a surface says: its blocks, in the order a reader reads them.
#[derive(Default)]
pub struct Doc {
    blocks: Vec<Block>,
}

impl Doc {
    pub fn new() -> Self {
        Doc::default()
    }

    /// This document with `block` after what it says so far.
    pub fn with(mut self, block: impl Into<Block>) -> Self {
        self.blocks.push(block.into());
        self
    }

    /// This document with each of `blocks` after what it says so far.
    pub fn with_all(mut self, blocks: impl IntoIterator<Item = Block>) -> Self {
        self.blocks.extend(blocks);
        self
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// What this document says, as blocks another document can hold.
    pub fn into_blocks(self) -> Vec<Block> {
        self.blocks
    }
}
