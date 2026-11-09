//! What `yunta test` says as its cases run: each case's row as it ends, in
//! the column every case's name lines up in, and the tally under them; and
//! how one case's run ended, which the pages about a pack say of its own
//! cases too. Running a case is its caller's.

use surco::components::{Check, Checklist, Found, Headline};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use surco::text::counted;
use yunta_core::step::NextStep;

use super::Page;
use crate::command;

/// How one case's run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseRun {
    /// It reached everything its `expect` says.
    Held,
    /// It ran, and fell short of its `expect` in each of these.
    Failed(Vec<String>),
    /// It could not run, for this.
    CouldNotRun(String),
}

impl CaseRun {
    /// Whether it held.
    pub fn held(&self) -> bool {
        matches!(self, CaseRun::Held)
    }

    /// What its row finds.
    fn found(&self) -> Found {
        match self {
            CaseRun::Held => Found::Holds,
            CaseRun::Failed(_) | CaseRun::CouldNotRun(_) => Found::Problem,
        }
    }

    /// What its row says: that it held, or how many problems it had and
    /// whether it ran.
    fn said(&self) -> String {
        match self {
            CaseRun::Held => "ok".to_string(),
            CaseRun::Failed(problems) => format!("failed: {}", counted(problems.len(), "error")),
            CaseRun::CouldNotRun(_) => "could not run: 1 error".to_string(),
        }
    }

    /// Each problem, as written: what fell short, or why it could not run.
    fn problems(&self) -> Vec<String> {
        match self {
            CaseRun::Held => Vec::new(),
            CaseRun::Failed(problems) => problems.clone(),
            CaseRun::CouldNotRun(error) => vec![error.clone()],
        }
    }
}

/// A case, by its name, and how its run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RanCase {
    pub name: String,
    pub run: CaseRun,
}

impl RanCase {
    /// Its row: what it found, by its name, with each problem under it.
    pub(super) fn check(&self) -> Check {
        Check::new(self.run.found(), self.name.as_str(), self.run.said())
            .details(self.run.problems())
    }
}

/// A case tested, said as it ends: what it found, in the column every case's
/// name lines up in, with what went wrong under it.
pub struct TestedCase<'a> {
    /// Every case's name, which sets the column the rows share.
    pub names: &'a [String],
    pub case: RanCase,
}

impl Page for TestedCase<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let mut row = Checklist::aligned(self.names.to_vec());
        row.push_check(self.case.check());
        Doc::new().with(row)
    }
}

/// The verdict under the rows it counts, as a run's closing comes under
/// what it showed while it ran: how many cases ran, and how many failed.
pub struct Tested {
    pub cases: usize,
    pub failures: usize,
}

impl Page for Tested {
    fn document(&self, _look: &Look) -> Doc {
        let tally = format!(
            "{}, {} failed",
            surco::text::counted(self.cases, "case"),
            self.failures
        );
        let mark = match self.failures {
            0 => Mark::Done,
            _ => Mark::Failed,
        };
        Doc::new().with(Headline::new(
            mark,
            command::line(&NextStep::Test { dir: None }),
            tally,
        ))
    }
}
