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

/// One check: what it found, what it checked, and what it found there.
pub struct Check {
    pub found: Found,
    pub subject: String,
    pub said: String,
}

/// The checks of one report, in the order they ran.
#[derive(Default)]
pub struct Checklist {
    pub(crate) checks: Vec<Check>,
}

impl Checklist {
    /// This list with one more check on it.
    pub fn push(&mut self, found: Found, subject: impl Into<String>, said: impl Into<String>) {
        self.checks.push(Check {
            found,
            subject: subject.into(),
            said: said.into(),
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

    /// Whether nothing on the list stops a run.
    /// Each check, in the order it was made.
    pub fn checks(&self) -> impl Iterator<Item = &Check> {
        self.checks.iter()
    }

    pub fn holds(&self) -> bool {
        self.checks
            .iter()
            .all(|check| check.found != Found::Problem)
    }
}
