//! What a set of checks found, one row each: the mark that says whether
//! it holds, what was checked, and what was found.

use crate::glyphs::Mark;

/// What one check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// It holds.
    Holds,
    /// Worth a reader's attention; nothing stops because of it.
    Caution,
    /// A run that reaches it stops there.
    Problem,
    /// There was nothing to check: what it would hold to was never
    /// asked for. Not a cross — nothing failed.
    NotApplicable,
}

impl Found {
    /// The mark a finding of this kind is drawn with.
    pub fn mark(self) -> Mark {
        match self {
            Found::Holds => Mark::Done,
            Found::Caution => Mark::Caution,
            Found::Problem => Mark::Failed,
            Found::NotApplicable => Mark::Absent,
        }
    }
}

/// One check: what it found, what it checked, what it found there, and
/// the lines that say more, each as written, under its row.
pub struct Check {
    pub found: Found,
    pub subject: String,
    pub said: String,
    pub details: Vec<String>,
}

/// The checks of one report, in the order they ran.
#[derive(Default)]
pub struct Checklist {
    pub(crate) checks: Vec<Check>,
    /// Every subject the report holds, this list's among them: a report
    /// drawn a row at a time, as each check comes, still lines its rows up.
    pub(crate) aligned: Vec<String>,
}

impl Checklist {
    /// An empty list whose subjects line up with every one of `subjects`:
    /// rows of one report drawn apart, as each check comes, line up.
    pub fn aligned(subjects: Vec<String>) -> Self {
        Checklist {
            checks: Vec::new(),
            aligned: subjects,
        }
    }

    /// This list with one more check on it.
    pub fn push(&mut self, found: Found, subject: impl Into<String>, said: impl Into<String>) {
        self.checks.push(Check {
            found,
            subject: subject.into(),
            said: said.into(),
            details: Vec::new(),
        });
    }

    /// This list with `check` on it.
    pub fn push_check(&mut self, check: Check) {
        self.checks.push(check);
    }

    /// This list with `checks` on it, in their order.
    pub fn extend(&mut self, checks: impl IntoIterator<Item = Check>) {
        self.checks.extend(checks);
    }

    /// Each check, in the order it was made.
    pub fn checks(&self) -> impl Iterator<Item = &Check> {
        self.checks.iter()
    }

    /// How many checks on the list found `found`.
    pub fn count(&self, found: Found) -> usize {
        self.checks
            .iter()
            .filter(|check| check.found == found)
            .count()
    }

    /// Whether nothing on the list stops a run.
    pub fn holds(&self) -> bool {
        self.checks
            .iter()
            .all(|check| check.found != Found::Problem)
    }
}
