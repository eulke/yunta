//! The media a document is drawn on.
//!
//! Each surface takes a [`Doc`] and gives what its medium shows: a
//! terminal, lines painted for a stream; a file or a forge's page,
//! Markdown. Every surface draws every block,
//! and draws the words a block says whole — the word carries the meaning,
//! so no medium may drop it.

mod markdown;
mod terminal;

pub use markdown::Markdown;
pub use terminal::Terminal;

use crate::doc::Doc;

/// A medium a document is drawn on.
pub trait Surface {
    /// What the medium shows.
    type Output;

    fn draw(&self, doc: &Doc<'_>) -> Self::Output;
}
