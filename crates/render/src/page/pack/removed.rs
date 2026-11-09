//! What `pack remove` answers with: that the pack is out of the project.

use surco::components::Headline;
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::PackRef;

use crate::page::Page;

/// A pack taken out of the project, as the person who asked for it reads it.
pub struct Removed<'a> {
    pub pack: &'a PackRef,
}

impl Page for Removed<'_> {
    fn document(&self, _look: &Look) -> Doc {
        Doc::new().with(Headline::new(
            Mark::Done,
            format!("pack {}", self.pack),
            "removed",
        ))
    }
}
