//! What `doctor` found, as a person reads it: whether this machine is ready,
//! each check — those that hold folded on a glance — and what to run for
//! more.
//!
//! What a check found is data, a [`Finding`] each, made where the check is
//! made; the words each says, and that a fix is said once under the first
//! row that needs it, are here (`finding`), so every interface that shows
//! `doctor` says the same.

mod finding;
mod session;
#[cfg(test)]
mod tests;

use surco::components::{Check, Furrow, Headline, Next};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::step::NextStep;

use super::checklist::{checklist, tallied};
use super::Page;
use crate::command;

pub use finding::{AdapterHealth, CheckoutsKeep, Finding, ForgeAnswer, PackLack, RemoteAnswer};
pub use session::{ProbeEnded, SessionOutcome};

/// What `doctor` found, as a person reads it: whether this machine is
/// ready, each check — those that hold folded on a glance — and what to
/// run for more.
pub struct Doctor<'a> {
    /// Whether `--session` opened a session per binding.
    pub session: bool,
    pub findings: &'a [Finding],
    /// Whether every check stands on its own row.
    pub full: bool,
}

/// Whether a machine is ready to run, as every check `doctor` made finds
/// it: the verdict its page, its document and the code it exits with say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readiness {
    /// Every check holds.
    Ready,
    /// A check does not hold.
    NotReady,
}

impl Readiness {
    /// The readiness `findings` find: ready only when every one holds.
    pub fn of(findings: &[Finding]) -> Readiness {
        match findings.iter().all(|finding| finding.check().holds()) {
            true => Readiness::Ready,
            false => Readiness::NotReady,
        }
    }

    /// Whether every check holds.
    pub fn holds(self) -> bool {
        self == Readiness::Ready
    }

    /// The word the verdict says.
    pub fn word(self) -> &'static str {
        match self {
            Readiness::Ready => "ready",
            Readiness::NotReady => "not ready",
        }
    }
}

/// Every finding as the row it is, in the order the checks ran: a fix is
/// said once, under the first row that needs it.
pub(crate) fn checks(findings: &[Finding]) -> Vec<Check> {
    let mut rows: Vec<Check> = Vec::new();
    for finding in findings {
        let check = finding.check();
        let given = check
            .fix
            .as_ref()
            .is_some_and(|fix| rows.iter().any(|earlier| earlier.fix.as_ref() == Some(fix)));
        rows.push(match given {
            true => Check { fix: None, ..check },
            false => check,
        });
    }
    rows
}

impl Doctor<'_> {
    /// Whether this machine is ready to run, and how many checks stop a
    /// run or are worth a look.
    fn headline(&self, checks: &[Check]) -> Headline {
        let readiness = Readiness::of(self.findings);
        let mark = match readiness.holds() {
            true => Mark::Done,
            false => Mark::Failed,
        };
        tallied(
            Headline::new(
                mark,
                command::line(&NextStep::Doctor {
                    sessions: false,
                    full: false,
                }),
                readiness.word(),
            ),
            checks,
        )
    }
}

impl Page for Doctor<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let rows = checks(self.findings);
        let listed = match self.full {
            true => checklist(&rows),
            false => checklist(&rows).folded("healthy"),
        };
        let mut next = Vec::new();
        if !self.session {
            next.push((
                command::line(&NextStep::Doctor {
                    sessions: true,
                    full: false,
                }),
                "opens a session per binding, a prompt each",
            ));
        }
        // A glance that folds leaves out what each check that holds found.
        if listed.folds() {
            next.push((
                command::line(&NextStep::Doctor {
                    sessions: false,
                    full: true,
                }),
                "every check on its own row, with what it found",
            ));
        }
        Doc::new()
            .with(self.headline(&rows))
            .with(Furrow(Vec::new()))
            .with(listed)
            .with(Next { steps: next })
    }
}
