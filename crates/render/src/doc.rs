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

use crate::blocks::{
    Checklist, Decision, Evidence, FailureDetail, Fields, Headline, Next, NodeTable,
};
use crate::ink::Line;

/// One part of a document.
pub enum Block<'a> {
    Headline(Headline),
    Fields(Fields),
    NodeTable(NodeTable),
    Evidence(Evidence),
    Failure(FailureDetail<'a>),
    Decision(Decision),
    Checklist(Checklist),
    Next(Next),
    /// Lines a surface composed itself, each span carrying its role.
    Lines(Vec<Line>),
}

macro_rules! block_from {
    ($($variant:ident($kind:ty)),* $(,)?) => {
        $(impl<'a> From<$kind> for Block<'a> {
            fn from(block: $kind) -> Self {
                Block::$variant(block)
            }
        })*
    };
}

block_from!(
    Headline(Headline),
    Fields(Fields),
    NodeTable(NodeTable),
    Evidence(Evidence),
    Failure(FailureDetail<'a>),
    Decision(Decision),
    Checklist(Checklist),
    Next(Next),
    Lines(Vec<Line>),
);

/// What a surface says: its blocks, in the order a reader reads them.
#[derive(Default)]
pub struct Doc<'a> {
    blocks: Vec<Block<'a>>,
}

impl<'a> Doc<'a> {
    pub fn new() -> Self {
        Doc::default()
    }

    /// This document with `block` after what it says so far.
    pub fn with(mut self, block: impl Into<Block<'a>>) -> Self {
        self.blocks.push(block.into());
        self
    }

    pub fn blocks(&self) -> &[Block<'a>] {
        &self.blocks
    }
}
