//! What a pack's own cases say, on the pages about the pack: none shipped,
//! shipped and not run, or run, with each that failed under them.

use surco::components::{Checklist, Fields};
use surco::doc::Block;
use surco::glyphs::Mark;
use surco::text::counted;

use crate::page::RanCase;

/// A pack's own cases, under its `.yunta/tests/` in the format `yunta test`
/// reads: none, counted, or run.
#[derive(Debug, Clone, PartialEq)]
pub enum Cases {
    /// The pack ships none.
    NoneShipped,
    /// It ships this many, not run: `add` runs them only when asked.
    NotRun { shipped: usize },
    /// Each ran, and how its run ended.
    Ran(Vec<RanCase>),
}

impl Cases {
    /// How many of them failed: none unless they ran.
    pub fn failed(&self) -> usize {
        self.failures().len()
    }

    /// Each case that ran and did not hold.
    fn failures(&self) -> Vec<&RanCase> {
        match self {
            Cases::Ran(cases) => cases.iter().filter(|case| !case.run.held()).collect(),
            Cases::NoneShipped | Cases::NotRun { .. } => Vec::new(),
        }
    }

    /// What they say, on the row they get: none shipped, how many were
    /// shipped and not run, or how many ran and how many failed.
    fn said(&self) -> String {
        match self {
            Cases::NoneShipped => "none shipped".to_string(),
            Cases::NotRun { shipped } => format!(
                "{} shipped, not run (pass --run-tests)",
                counted(*shipped, "case")
            ),
            Cases::Ran(cases) => {
                format!("{}, {} failed", counted(cases.len(), "case"), self.failed())
            }
        }
    }

    /// The mark a page about the pack opens with: what the page did held,
    /// and a failed case is worth a look.
    pub(super) fn mark(&self) -> Mark {
        match self.failed() {
            0 => Mark::Done,
            _ => Mark::Caution,
        }
    }

    /// Their row, and a row for each case that failed with every problem it
    /// had under it, each line as written, as `yunta test` lists them.
    pub(super) fn blocks(&self) -> Vec<Block> {
        let failures = self.failures();
        let mut failed = Checklist::default();
        for case in &failures {
            failed.push_check(case.check());
        }
        let failed = (!failures.is_empty()).then(|| failed.into());
        std::iter::once(Fields::new().push_if("tests", self.said()).into())
            .chain(failed)
            .collect()
    }
}
