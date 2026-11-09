//! The verdict on a run's evidence, as a person reads it: whether it is
//! intact, and each check in the order it ran.

use surco::components::{Check, Headline};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::RunId;

use super::checklist::{checklist, tallied};
use super::Page;

/// The verdict on a run's evidence, as a person reads it: whether it is
/// intact, and each check in the order it ran.
pub struct Verified<'a> {
    pub run_id: &'a RunId,
    pub checks: &'a [Check],
}

impl Page for Verified<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let mark = match self.holds() {
            true => Mark::Done,
            false => Mark::Failed,
        };
        let headline = tallied(
            Headline::new(
                mark,
                format!("run {}", self.run_id.handle()),
                Verified::word(self.holds()),
            ),
            self.checks,
        );
        Doc::new().with(headline).with(checklist(self.checks))
    }
}

impl Verified<'_> {
    /// Whether every check holds.
    fn holds(&self) -> bool {
        self.checks.iter().all(Check::holds)
    }

    /// The word a verdict says: `intact` when every check holds, `broken` when one does not.
    pub fn word(holds: bool) -> &'static str {
        match holds {
            true => "intact",
            false => "broken",
        }
    }
}
