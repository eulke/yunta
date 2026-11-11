//! What `pack new` answers with: the pack it wrote, where it is, and what
//! its check found — the files alone where something stopped the check.

use surco::components::{Fields, Furrow, Headline, Problem};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::look::Look;
use surco::text::counted;
use yunta_core::PackRef;

use super::Cases;
use crate::page::Page;

/// A pack `pack new` wrote, as a person reads it: where it is, and what its
/// check found; the files alone where something stopped the check.
pub struct NewPack<'a> {
    pub pack: &'a PackRef,
    /// Where the pack is, as a reader in this directory names it.
    pub at: &'a str,
    /// What its check found; `None` where something stopped the check.
    pub checked: Option<&'a NewPackCheck>,
}

impl Page for NewPack<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let mark = self.checked.map_or(Mark::Done, NewPackCheck::mark);
        Doc::new()
            .with(Headline::new(
                mark,
                format!("pack {}", self.pack),
                "written",
            ))
            .with(Furrow(Vec::new()))
            .with(Fields::new().push_if("at", self.at))
            .with_all(self.checked.map(NewPackCheck::blocks).unwrap_or_default())
    }
}

/// What the check `pack new` runs on the pack it wrote found: the problems
/// that stop its example workflow, or, when none does, what the pack's own
/// cases say.
#[derive(Debug)]
pub enum NewPackCheck {
    /// The example workflow, at `workflow` in the pack, does not check:
    /// each problem, as `check` says it.
    Refused {
        workflow: String,
        problems: Vec<Problem>,
    },
    /// It checks, and the pack's cases ran.
    Tested(Cases),
}

impl NewPackCheck {
    /// The mark the page opens with: worth a look where the example does not
    /// check, else the case's.
    fn mark(&self) -> Mark {
        match self {
            NewPackCheck::Refused { .. } => Mark::Caution,
            NewPackCheck::Tested(cases) => cases.mark(),
        }
    }

    /// What the page says of it: how many problems stop the example, and
    /// each; or what the pack's case says.
    fn blocks(&self) -> Vec<Block> {
        match self {
            NewPackCheck::Refused { workflow, problems } => std::iter::once(
                Fields::new()
                    .push_if(
                        "check",
                        format!("{} in {workflow}", counted(problems.len(), "error")),
                    )
                    .into(),
            )
            .chain(problems.iter().cloned().map(Block::from))
            .collect(),
            NewPackCheck::Tested(cases) => cases.blocks(),
        }
    }
}
