//! What `yunta open` answers with, as a person reads it.

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
            Some(false) => doc.with(Note(
                "nothing here could open it: open that file in a browser.".to_string(),
            )),
            _ => doc,
        }
    }
}
