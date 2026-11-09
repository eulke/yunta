//! What `doctor` found, as a person reads it: whether this machine is ready,
//! each check — those that hold folded on a glance — and what to run for
//! more.

use surco::components::{Check, Furrow, Headline, Next};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::step::NextStep;

use super::checklist::{checklist, tallied};
use super::Page;
use crate::command;

/// What `doctor` found, as a person reads it: whether this machine is
/// ready, each check — those that hold folded on a glance — and what to
/// run for more.
pub struct Doctor<'a> {
    /// Whether `--session` opened a session per binding.
    pub session: bool,
    pub checks: &'a [Check],
    /// Whether every check stands on its own row.
    pub full: bool,
}

impl Doctor<'_> {
    /// Whether every check holds.
    fn holds(&self) -> bool {
        self.checks.iter().all(Check::holds)
    }

    /// The word a doctor verdict says: `ready` when every check holds, `not ready` when one does not.
    pub fn word(holds: bool) -> &'static str {
        match holds {
            true => "ready",
            false => "not ready",
        }
    }

    /// Whether this machine is ready to run, and how many checks stop a
    /// run or are worth a look.
    fn headline(&self) -> Headline {
        let mark = match self.holds() {
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
                Doctor::word(self.holds()),
            ),
            self.checks,
        )
    }
}

impl Page for Doctor<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let checks = match self.full {
            true => checklist(self.checks),
            false => checklist(self.checks).folded("healthy"),
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
        if checks.folds() {
            next.push((
                command::line(&NextStep::Doctor {
                    sessions: false,
                    full: true,
                }),
                "every check on its own row, with what it found",
            ));
        }
        Doc::new()
            .with(self.headline())
            .with(Furrow(Vec::new()))
            .with(checks)
            .with(Next { steps: next })
    }
}
