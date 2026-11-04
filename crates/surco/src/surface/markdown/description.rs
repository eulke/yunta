//! A forge's page: the description of a pull request, drawn as a Markdown
//! file is, with what it leaves out said by whoever draws it.

use crate::doc::Doc;
use crate::surface::Surface;

/// A forge's page: the description of a pull request, under the title the
/// forge draws above it. It is drawn as a Markdown file is, except that its
/// first headline is a line, since the forge's title heads the page, and a
/// document's title is a heading of the second level; and it leaves out
/// what `leaves` names, which whoever draws it says at its top.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Description {
    pub leaves: Leaves,
}

/// What a description leaves out of the blocks it draws, each the one
/// before it and more: what a reader misses least goes first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Leaves {
    /// Nothing: every block whole.
    #[default]
    Nothing,
    /// The lines of each code block: its title, what it is for and what is
    /// said under it stay.
    CodeLines,
    /// Those, and what each fold holds: its title stays, as a line.
    Folds,
}

impl Surface for Description {
    /// The description, its blocks separated by a blank line.
    type Output = String;

    fn draw(&self, doc: &Doc) -> String {
        super::drawn(
            doc,
            super::Page {
                description: true,
                leaves: self.leaves,
                folded: false,
            },
        )
    }
}
