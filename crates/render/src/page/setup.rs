//! `yunta init -i`, before anything is written: what setting a project up found in a
//! repository and what it would write, the page a person reads while they answer for
//! its name, its base branch and its runners. What it wrote is [`Initialized`]'s.
//!
//! [`Initialized`]: super::Initialized

use std::path::Path;

use surco::components::{Fields, Furrow, Headline};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::step::NextStep;
use yunta_core::{Detected, ProbedAdapter, RunnerName};

use super::initialized::{adapters, found};
use super::Page;
use crate::command;

/// What setting a project up found, and what it would write.
pub struct Setup<'a> {
    /// The repository being set up.
    pub repo: &'a Path,
    /// The name it is written under: as answered, or as detected until it is.
    pub name: &'a str,
    /// The branch its runs start from: as answered, or as detected until it is.
    pub base_branch: &'a str,
    /// What the repository was found to have.
    pub detected: &'a Detected,
    /// Every adapter this build knows, probed.
    pub probed: &'a [ProbedAdapter],
    /// The runner roles its workflows propose.
    pub proposed: &'a [RunnerName],
    /// What setting it up writes, by its path in the repository.
    pub writes: &'a [String],
}

impl Page for Setup<'_> {
    /// The page: what is still to write, where, as the headline; the name, the base
    /// branch, what it writes and what the repository was found to have, as fields;
    /// each adapter this machine answers for, as a check; and the runners proposed.
    fn document(&self, _look: &Look) -> Doc {
        let headline = Headline::new(Mark::Absent, command::line(&NextStep::Init), "to write")
            .fact(format!("in {}", self.repo.display()));
        let fields = Fields::new()
            .push_if("name", self.name)
            .push_if("base branch", self.base_branch)
            .push_if("writes", self.writes.join(", "));
        let proposed = surco::text::listed(self.proposed.iter().map(RunnerName::as_str));
        Doc::new()
            .with(headline)
            .with(Furrow(Vec::new()))
            .with(found(fields, self.detected))
            .with(adapters(self.probed))
            .with(Fields::new().push_if("runners", proposed))
    }
}
