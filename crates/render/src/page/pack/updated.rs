//! What `pack update` answers with: the ref the pack is at now, and the
//! commit that ref resolved to.

use surco::components::Headline;
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::{CommitSha, PackRef};

use crate::page::Page;

/// A pack moved to a new ref, as the person who asked for it reads it: the
/// ref it is at now, and the commit that ref resolved to.
pub struct Updated<'a> {
    pub pack: &'a PackRef,
    /// The ref it is at now.
    pub to: &'a str,
    /// The commit that ref resolved to.
    pub commit: &'a CommitSha,
}

impl Page for Updated<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let headline = Headline::new(Mark::Done, format!("pack {}", self.pack), "updated")
            .fact(format!("to {}", self.to))
            .fact(self.commit.abbreviated());
        Doc::new().with(headline)
    }
}
