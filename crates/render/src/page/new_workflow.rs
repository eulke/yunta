//! What `yunta new` answers with, as a person reads it: the file it wrote,
//! and the verdict `yunta check` reaches on it, said under it.

use std::path::Path;

use surco::components::{Fields, Furrow, Headline, Next};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::step::{Arg, NextStep};

use super::checked::{check_again, file_named};
use super::{Diagnosis, Page};
use crate::command;

/// What `new` answers with: the file it wrote, and the verdict `yunta check`
/// reaches on it, said under it; the file alone where something stopped the
/// check.
pub struct NewWorkflow<'a> {
    /// The workflow's name, which the command that runs it takes.
    pub name: &'a str,
    /// The shape its skeleton took, by the name `--shape` reads.
    pub shape: &'a str,
    pub path: &'a Path,
    /// Where the file is, as a reader in this directory names it.
    pub at: &'a str,
    pub diagnosis: Option<&'a Diagnosis>,
}

impl Page for NewWorkflow<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let written = |mark| Headline::new(mark, file_named(self.path), "written").fact(self.shape);
        let Some(diagnosis) = self.diagnosis else {
            return Doc::new()
                .with(written(Mark::Done))
                .with(Furrow(Vec::new()))
                .with(Fields::new().push_if("at", self.at));
        };
        let (mark, steps) = match diagnosis.errors() {
            0 => (Mark::Done, vec![self.runs_it()]),
            _ => (Mark::Caution, vec![check_again(self.at)]),
        };
        Doc::new()
            .with(written(mark).fact(diagnosis.said()))
            .with(Furrow(Vec::new()))
            .with(Fields::new().push_if("at", self.at))
            .with_all(diagnosis.blocks())
            .with(Next { steps })
    }
}

impl NewWorkflow<'_> {
    /// The step that runs the workflow just written.
    fn runs_it(&self) -> (String, &'static str) {
        let run = NextStep::Run {
            workflow: Arg::Given(self.name.to_string()),
        };
        (command::line(&run), "runs it")
    }
}
