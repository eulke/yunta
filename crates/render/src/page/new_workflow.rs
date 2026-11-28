//! What `yunta new` answers with, as a person reads it: the file it wrote,
//! and the verdict `yunta check` reaches on it, said under it.

use std::path::Path;

use surco::components::{Fields, Furrow, Headline};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::step::{Arg, NextStep};

use super::checked::{check_again, file_named};
use super::{Diagnosis, Page};
use crate::command::{self, SaidStep};

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
        let (mark, step) = match diagnosis.errors() {
            0 => (Mark::Done, self.runs_it()),
            _ => (Mark::Caution, check_again(self.at)),
        };
        Doc::new()
            .with(written(mark).fact(diagnosis.said()))
            .with(Furrow(Vec::new()))
            .with(Fields::new().push_if("at", self.at))
            .with_all(diagnosis.blocks())
            .with(command::next([step]))
    }
}

impl NewWorkflow<'_> {
    /// The step that starts a run of the workflow just written.
    fn runs_it(&self) -> SaidStep {
        SaidStep::of(&NextStep::Run {
            workflow: Arg::Given(self.name.to_string()),
        })
    }
}
