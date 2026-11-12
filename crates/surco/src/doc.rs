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
    Attention, Card, Checklist, ChoiceList, Code, Concern, Decision, Desk, Diffstat, Evidence,
    Fields, Furrow, Gist, Headline, Legend, Marked, Next, NodeTable, Note, Paths, Problem, Prose,
    Refusal, Ribbon, Runs, Said, Section, Table, TypedAnswer,
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

/// One part of a document. Not a [`Screen`](crate::components::Screen),
/// which holds documents, each in its place.
pub enum Block {
    Headline(Headline),
    /// Where each node of the run stands, under the headline.
    Furrow(Furrow),
    /// Where each node of the run stands, under its name: a screen's signature.
    Ribbon(Ribbon),
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
    /// What a change came to, file by file.
    Diffstat(Diffstat),
    Decision(Decision),
    /// A decision put to a person while they answer it: its question, then its
    /// options with one pointed at, the words being said for the one chosen, or
    /// a round of questions; and its keys.
    Card(Card),
    /// A plan read at its desk while a person decides on it: what it is, the places
    /// they can stand on, what is being read, and what is being asked.
    Desk(Desk),
    /// A line asked of a person and the answer typed under it.
    TypedAnswer(TypedAnswer),
    /// A list a person picks one value from, narrowed by what they type.
    ChoiceList(ChoiceList),
    Checklist(Checklist),
    /// A problem found in a text, pointed at in its line.
    Problem(Problem),
    /// What a command would not do.
    Refusal(Refusal),
    Next(Next),
    /// The keys of what is on screen, each with what it does.
    Legend(Legend),
    /// What needs a person, and how they answer it.
    Attention(Attention),
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
    /// Lines a surface composed, one to a row, which a pointer moves along: a row that
    /// passes the edge it is drawn at is cut there with the ellipsis, never folded onto
    /// the row under it. Not [`Block::Lines`], which a pane's edge folds.
    Rows(Vec<Line>),
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
    Ribbon(Ribbon),
    Gist(Gist),
    Fields(Fields),
    Table(Table),
    Nodes(NodeTable),
    Runs(Runs),
    Evidence(Evidence),
    Said(Said),
    Paths(Paths),
    Diffstat(Diffstat),
    Decision(Decision),
    Card(Card),
    Desk(Desk),
    TypedAnswer(TypedAnswer),
    ChoiceList(ChoiceList),
    Checklist(Checklist),
    Problem(Problem),
    Refusal(Refusal),
    Next(Next),
    Legend(Legend),
    Attention(Attention),
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

/// A block a person answers while it is held: what a terminal fits to the rows it has,
/// with the cursor where it is typed into. Not [`Block`], which is whatever a document
/// says; a block is one of these only as [`Block::prompt`] sees it.
pub enum Prompt<'a> {
    Card(&'a Card),
    Desk(&'a Desk),
    TypedAnswer(&'a TypedAnswer),
    ChoiceList(&'a ChoiceList),
}

impl Block {
    /// This block as the prompt a person answers, when it is one: a card, a desk, a typed
    /// answer or a choice list.
    pub fn prompt(&self) -> Option<Prompt<'_>> {
        match self {
            Block::Card(card) => Some(Prompt::Card(card)),
            Block::Desk(desk) => Some(Prompt::Desk(desk)),
            Block::TypedAnswer(asked) => Some(Prompt::TypedAnswer(asked)),
            Block::ChoiceList(list) => Some(Prompt::ChoiceList(list)),
            _ => None,
        }
    }
}

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

    /// Where the decision this document waits on stands: the last block that is
    /// one, or a section that holds one. A page draws it apart, and what it is
    /// about goes before it.
    pub fn decision_at(&self) -> Option<usize> {
        self.blocks.iter().rposition(|block| match block {
            Block::Decision(_) => true,
            Block::Section(section) => section
                .blocks
                .iter()
                .any(|inner| matches!(inner, Block::Decision(_))),
            _ => false,
        })
    }

    /// Where the prompt a person answers in this document stands: its first block that is
    /// a [`Prompt`]. A pane holds it fitted to the rows the other blocks leave.
    pub fn prompt_at(&self) -> Option<usize> {
        self.blocks
            .iter()
            .position(|block| block.prompt().is_some())
    }

    /// What this document says, as blocks another document can hold.
    pub fn into_blocks(self) -> Vec<Block> {
        self.blocks
    }
}
