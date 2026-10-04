//! What the lineage's measurement says about the suite every task is held
//! to, as one invocation learns it.
//!
//! A run measuring aside starts its loop before the suite has answered.
//! Its tasks carry the suite as a guard from the start, and only judging
//! that guard needs the answer: a check whose own criteria pass waits for
//! the measurement there, and holds the task to the suite only when it
//! passed before the run changed anything — a suite already red holds
//! nothing. A check before the work leaves the guard out while it waits:
//! the measurement is that answer, on the tree the task starts from.

use tokio_util::sync::CancellationToken;
use yunta_core::Criterion;

/// What this invocation knows of the measurement.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Known {
    /// The suite `0` is being measured, and has not answered.
    Pending(String),
    /// The suite `0` answered; it holds tasks when it passed.
    Measured { command: String, passed: bool },
    /// No suite holds tasks, or the measurement stopped without an answer.
    Nothing,
}

/// The suite a run's tasks are held to, as the measurement settles it.
pub struct SuiteGate {
    known: tokio::sync::watch::Sender<Known>,
}

impl Default for SuiteGate {
    fn default() -> Self {
        SuiteGate {
            known: tokio::sync::watch::Sender::new(Known::Nothing),
        }
    }
}

impl SuiteGate {
    /// The suite `command` is being measured.
    pub fn expect(&self, command: &str) {
        self.known
            .send_replace(Known::Pending(command.trim().to_string()));
    }

    /// The suite `command` answered, passing or not.
    pub fn settle(&self, command: &str, passed: bool) {
        self.known.send_replace(Known::Measured {
            command: command.trim().to_string(),
            passed,
        });
    }

    /// The measurement stopped without an answer: a guard waiting on it
    /// runs the suite itself, as one would with no measurement at all.
    pub fn give_up(&self) {
        self.known.send_if_modified(|known| match known {
            Known::Pending(_) => {
                *known = Known::Nothing;
                true
            }
            _ => false,
        });
    }

    /// The suite that holds a run's tasks: the one its lineage `measured`
    /// green, or the one still being measured while the run goes on.
    pub fn holding(
        &self,
        measured: Option<&yunta_core::events::BaselineCapturedPayload>,
    ) -> Option<String> {
        match (measured, &*self.known.borrow()) {
            (Some(measured), _) => crate::tasks::suite_of(Some(measured)).map(str::to_string),
            (None, Known::Pending(command)) => Some(command.clone()),
            (None, _) => None,
        }
    }

    /// Whether `criterion` is the suite, still being measured.
    pub fn pending(&self, criterion: &Criterion) -> bool {
        matches!(&*self.known.borrow(), Known::Pending(command) if is(criterion, command))
    }

    /// `guards`, as the measurement leaves them: waiting for it when one of
    /// them is the suite still being measured, and without the suite when
    /// it was red before the run changed anything. `cancel` stops the wait,
    /// leaving every guard in.
    pub async fn judging(
        &self,
        mut guards: Vec<Criterion>,
        cancel: &CancellationToken,
    ) -> Vec<Criterion> {
        let mut known = self.known.subscribe();
        loop {
            let now = known.borrow_and_update().clone();
            match now {
                Known::Pending(ref command) if guards.iter().any(|guard| is(guard, command)) => {
                    tokio::select! {
                        changed = known.changed() => {
                            if changed.is_err() {
                                return guards;
                            }
                        }
                        () = cancel.cancelled() => return guards,
                    }
                }
                Known::Measured {
                    command,
                    passed: false,
                } => {
                    guards.retain(|guard| !is(guard, &command));
                    return guards;
                }
                _ => return guards,
            }
        }
    }
}

fn is(criterion: &Criterion, command: &str) -> bool {
    criterion.is_guard() && criterion.cmd.trim() == command
}
