//! What `doctor` found, as a person reads it: whether this machine is ready,
//! each check — those that hold folded on a glance — and what to run for
//! more.
//!
//! What a check found is data, a [`Finding`] each, made where the check is
//! made; the words each says, and that a fix is said once under the first
//! row that needs it, are here (`finding`), so every interface that shows
//! `doctor` says the same. Beside a new run the same findings stand at a
//! glance ([`glance`]): what this machine answers for, each fact that holds
//! in fewer words; where the panel gives way, what of it stands in the way of
//! the chosen workflow's run stays ([`in_the_way`]). On the application's screen the page
//! names what it checked, `this machine`, where the printed page names its command, and
//! leaves what to run to the palette ([`Doctor::on_screen`]).

mod finding;
mod pool;
mod session;
#[cfg(test)]
mod tests;

use surco::components::{Check, Checklist, Furrow, Headline};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::step::NextStep;

use super::checklist::{checklist, tallied};
use super::Page;
use crate::command::{self, SaidStep};

pub use finding::{AdapterHealth, CheckoutsKeep, Finding, ForgeAnswer, PackLack, RemoteAnswer};
pub use pool::PoolKept;
pub use session::{NamedBy, ProbeEnded, SessionOutcome};

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
    said_once(findings.iter().map(Finding::check))
}

/// What `findings` find of this machine at a glance beside a new run, a row
/// for each check the glance shows ([`Finding::glanced`]), in the order the
/// checks ran: a fix is said once, under the first row that needs it.
pub fn glance(findings: &[Finding]) -> Vec<Check> {
    said_once(findings.iter().filter_map(Finding::glanced))
}

/// What of the glance stands in the way of a run of the catalog's workflow
/// `workflow`: each row that does not hold of a finding that bears on that run
/// ([`Finding::bears_on`]), as [`glance`] says it. What a new run too narrow
/// for its panel keeps in sight.
pub fn in_the_way(findings: &[Finding], workflow: &str) -> Vec<Check> {
    let bearing = findings.iter().filter(|finding| finding.bears_on(workflow));
    let rows = bearing.filter_map(Finding::glanced);
    said_once(rows.filter(|row| !row.holds()))
}

/// `checks`, in their order, each fix said under the first row that needs it
/// and left out of every row after.
fn said_once(checks: impl Iterator<Item = Check>) -> Vec<Check> {
    let mut rows: Vec<Check> = Vec::new();
    for check in checks {
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
    /// Whether `subject` is ready to run, and how many checks stop a run or are worth a
    /// look.
    fn headline(&self, subject: impl Into<String>, checks: &[Check]) -> Headline {
        let readiness = Readiness::of(self.findings);
        let mark = match readiness.holds() {
            true => Mark::Done,
            false => Mark::Failed,
        };
        tallied(Headline::new(mark, subject, readiness.word()), checks)
    }

    /// `rows`, every check on its own row, or the checks that hold folded at a glance.
    fn listed(&self, rows: &[Check]) -> Checklist {
        match self.full {
            true => checklist(rows),
            false => checklist(rows).folded("healthy"),
        }
    }

    /// The page as the application's screen tells it: the headline names what was checked,
    /// `this machine`, where the printed page names its command, and the checks stand under
    /// it as one list of what the machine answers for, close under the row the checks that
    /// hold fold into; the commands that go further are the palette's, so no next step
    /// stands.
    pub fn on_screen(&self) -> Doc {
        let rows = checks(self.findings);
        Doc::new()
            .with(self.headline(crate::screen::THIS_MACHINE, &rows))
            .with(Furrow(Vec::new()))
            .with(self.listed(&rows).close_under_its_fold())
    }
}

impl Page for Doctor<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let rows = checks(self.findings);
        let listed = self.listed(&rows);
        let mut next = Vec::new();
        if !self.session {
            next.push(SaidStep::of(&NextStep::Doctor {
                sessions: true,
                full: false,
            }));
        }
        // A glance that folds leaves out what each check that holds found.
        if listed.folds() {
            next.push(SaidStep::of(&NextStep::Doctor {
                sessions: false,
                full: true,
            }));
        }
        let command = command::line(&NextStep::Doctor {
            sessions: false,
            full: false,
        });
        Doc::new()
            .with(self.headline(command, &rows))
            .with(Furrow(Vec::new()))
            .with(listed)
            .with(command::next(next))
    }
}
