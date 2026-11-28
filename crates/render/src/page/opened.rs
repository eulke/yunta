//! What `yunta open` answers with, as a person reads it, and what a person is told where
//! nothing here could open a page or a link for them ([`unopened`]); and the same page as a
//! screen shows it once it is written, under a headline.

use surco::components::{Fields, Furrow, Headline, Note};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;

use super::Page;

/// What a screen calls a run's page written for a browser, in the headline that says it is
/// written.
const PAGE_FOR_A_BROWSER: &str = "the run's page for a browser";

/// Where a run's page was written, and, when nothing here could open it,
/// that the person opens it themselves.
pub struct Opened<'a> {
    pub page: &'a str,
    pub opened: Option<bool>,
}

impl Opened<'_> {
    /// The page as a screen shows it once it is written: a headline that says the run's page
    /// for a browser is written, over where it is and, where nothing here could open it, that
    /// the person opens it themselves. Not [`Opened::document`](Page::document), what
    /// `yunta open` prints.
    pub fn on_screen(&self) -> Doc {
        let written = Headline::new(Mark::Done, PAGE_FOR_A_BROWSER, "written");
        let page = self.written();
        Doc::new()
            .with(written)
            .with(Furrow(Vec::new()))
            .with_all(page.into_blocks())
    }

    /// Where the page was written, and that the person opens it themselves where nothing here
    /// could.
    fn written(&self) -> Doc {
        let doc = Doc::new().with(Fields::new().push_literal("page", self.page.to_string()));
        match self.opened {
            Some(false) => doc.with(Note(unopened("that file"))),
            _ => doc,
        }
    }
}

impl Page for Opened<'_> {
    fn document(&self, _look: &Look) -> Doc {
        self.written()
    }
}

/// What a person is told where nothing here could open `what` for them, a page or a link:
/// to open it in a browser themselves.
pub fn unopened(what: &str) -> String {
    format!("nothing here could open it: open {what} in a browser.")
}
