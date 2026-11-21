//! What `yunta open` answers with, as a person reads it, and what a person is told where
//! nothing here could open a page or a link for them ([`unopened`]).

use surco::components::{Fields, Note};
use surco::doc::Doc;
use surco::look::Look;

use super::Page;

/// Where a run's page was written, and, when nothing here could open it,
/// that the person opens it themselves.
pub struct Opened<'a> {
    pub page: &'a str,
    pub opened: Option<bool>,
}

impl Page for Opened<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let doc = Doc::new().with(Fields::new().push_literal("page", self.page.to_string()));
        match self.opened {
            Some(false) => doc.with(Note(unopened("that file"))),
            _ => doc,
        }
    }
}

/// What a person is told where nothing here could open `what` for them, a page or a link:
/// to open it in a browser themselves.
pub fn unopened(what: &str) -> String {
    format!("nothing here could open it: open {what} in a browser.")
}
