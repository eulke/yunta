//! What a set of checks found: a row each, with the mark that says whether
//! it holds, what was checked and what was found, and under the row what
//! that means and how to make it hold. A list that folds draws the checks
//! that hold as one row.

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

/// One check: what it found, what it checked and what it found there;
/// what that means and how to make it hold, when the check knows; what
/// names it where the checks that hold fold into one row; and the lines
/// that say more, each as written, under its row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub found: Found,
    pub subject: String,
    pub said: String,
    /// What the finding means, when the finding alone does not say it.
    pub so: Option<String>,
    /// How to make it hold, when the check knows.
    pub fix: Option<Fix>,
    /// What names it in the one row the checks that hold fold into; its
    /// subject when there is none.
    pub brief: Option<String>,
    pub details: Vec<String>,
}

/// How to make a finding hold: what to do, and the lines a person pastes
/// to do it, when it takes some, each as written.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Fix {
    pub said: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub paste: Vec<String>,
}

impl Fix {
    /// A fix its sentence says whole.
    pub fn said(said: impl Into<String>) -> Self {
        Fix {
            said: said.into(),
            paste: Vec::new(),
        }
    }

    /// The blocks a page draws it with when it stands under no check: its
    /// sentence as the heading the lines to paste hang from, or as prose
    /// when there is nothing to paste.
    pub fn blocks(&self) -> Vec<crate::doc::Block> {
        use crate::doc::Block;
        match self.paste.is_empty() {
            true => vec![super::Prose(self.said.clone()).into()],
            false => vec![
                Block::Heading(self.said.clone()),
                super::Said(self.paste.clone()).into(),
            ],
        }
    }
}

/// The one row the checks that hold fold into: how many there are, beside
/// the word the list folds them under, and what names each.
pub(crate) struct Fold<'a> {
    pub(crate) subject: String,
    pub(crate) names: Vec<&'a str>,
}

impl Check {
    /// A check that found `found` of `subject`, saying `said`, with nothing
    /// more to say.
    pub fn new(found: Found, subject: impl Into<String>, said: impl Into<String>) -> Self {
        Check {
            found,
            subject: subject.into(),
            said: said.into(),
            so: None,
            fix: None,
            brief: None,
            details: Vec::new(),
        }
    }

    /// This check with what its finding means.
    pub fn so(mut self, so: impl Into<String>) -> Self {
        self.so = Some(so.into());
        self
    }

    /// This check with how to make it hold.
    pub fn fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }

    /// This check with what names it where the checks that hold fold.
    pub fn brief(mut self, brief: impl Into<String>) -> Self {
        self.brief = Some(brief.into());
        self
    }

    /// This check with `details` under its row.
    pub fn details(mut self, details: Vec<String>) -> Self {
        self.details = details;
        self
    }
}

/// The checks of one report, in the order they ran.
#[derive(Default)]
pub struct Checklist {
    pub(crate) checks: Vec<Check>,
    /// Every subject the report holds, this list's among them: a report
    /// drawn a row at a time, as each check comes, still lines its rows up.
    pub(crate) aligned: Vec<String>,
    /// The word the checks that hold fold into one row under, beside how
    /// many there are; every check has its own row when there is none.
    pub(crate) folded: Option<String>,
}

impl Checklist {
    /// An empty list whose subjects line up with every one of `subjects`:
    /// rows of one report drawn apart, as each check comes, line up.
    pub fn aligned(subjects: Vec<String>) -> Self {
        Checklist {
            aligned: subjects,
            ..Checklist::default()
        }
    }

    /// This list with the checks that hold folded into one row that names
    /// each, `word` beside how many there are: `4 healthy`.
    pub fn folded(mut self, word: impl Into<String>) -> Self {
        self.folded = Some(word.into());
        self
    }

    /// Whether the list folds the checks that hold into one row: it asks
    /// to, and some check holds.
    pub fn folds(&self) -> bool {
        self.folded.is_some() && self.checks.iter().any(|check| check.found == Found::Holds)
    }

    /// The row the checks that hold fold into, when the list folds.
    pub(crate) fn fold(&self) -> Option<Fold<'_>> {
        let word = self.folded.as_deref().filter(|_| self.folds())?;
        let names: Vec<&str> = self
            .checks
            .iter()
            .filter(|check| check.found == Found::Holds)
            .map(|check| check.brief.as_deref().unwrap_or(&check.subject))
            .collect();
        Some(Fold {
            subject: format!("{} {word}", names.len()),
            names,
        })
    }

    /// The checks with a row of their own: every one, or the ones that do
    /// not hold when the list folds the rest.
    pub(crate) fn unfolded(&self) -> impl Iterator<Item = &Check> {
        let folds = self.folds();
        self.checks
            .iter()
            .filter(move |check| !folds || check.found != Found::Holds)
    }

    /// This list with one more check on it.
    pub fn push(&mut self, found: Found, subject: impl Into<String>, said: impl Into<String>) {
        self.checks.push(Check::new(found, subject, said));
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
