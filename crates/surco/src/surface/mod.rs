//! The media a document is drawn on.
//!
//! Each surface takes a [`Doc`] and gives what its medium shows: a
//! terminal, lines painted for a stream; a file or a forge's page,
//! Markdown. Every surface draws every block,
//! and draws the words a block says whole — the word carries the meaning,
//! so no medium may drop it.

mod diff;
mod markdown;
mod terminal;

pub use markdown::{Description, Leaves, Markdown};
pub use terminal::Terminal;

use crate::components::Code;
use crate::doc::Doc;
use crate::glyphs::Glyphs;

/// A medium a document is drawn on.
pub trait Surface {
    /// What the medium shows.
    type Output;

    fn draw(&self, doc: &Doc) -> Self::Output;
}

/// Where `code` is, as the parts its title says it in: its file, then each
/// thing in it after the sign `glyphs` puts before what stands inside another.
/// A line too narrow for all of them breaks between two parts.
pub(in crate::surface) fn code_title(code: &Code, glyphs: Glyphs) -> Vec<String> {
    std::iter::once(code.at.clone())
        .chain(
            code.within
                .iter()
                .map(|inner| format!("{} {inner}", glyphs.within())),
        )
        .collect()
}
