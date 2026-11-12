//! What `yunta close` answers with, as a person reads it.

use surco::components::{Gist, Headline};
use surco::doc::Doc;
use surco::look::Look;
use surco::words::Standing;
use yunta_core::step::NextStep;
use yunta_core::{Responder, RunId};

use super::Page;
use crate::command;

/// A close, as the person who asked for it reads it: the word the run
/// stands in, who closed it, and what stays until `gc`.
pub struct Closed<'a> {
    pub run_id: &'a RunId,
    pub word: Standing,
    pub by: &'a Responder,
}

impl Page for Closed<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let headline = Headline::new(
            self.word.mark(),
            format!("run {}", self.run_id.handle()),
            self.word.to_string(),
        );
        let gist = format!(
            "closed by {}: its checkouts went back to the project, and its branch stays until \
             `{}` removes it",
            self.by,
            command::line(&NextStep::Gc { dry_run: false })
        );
        Doc::new().with(headline).with(Gist::new(gist))
    }
}
