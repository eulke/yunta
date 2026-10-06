//! What the lineage's measurement says about the commands every task is
//! held to — its suite, and each command `baseline.held` names — as one
//! invocation learns it.
//!
//! A run measuring aside starts its loop before those commands have
//! answered. Its tasks carry them as guards from the start, and only
//! judging such a guard needs the answer: a check whose own criteria pass
//! waits for the measurement there, and holds the task to a command only
//! when it passed before the run changed anything — a command already red
//! holds nothing. A check before the work leaves the guard out while it
//! waits: the measurement is that answer, on the tree the task starts from.

use std::collections::{BTreeMap, BTreeSet};

use tokio_util::sync::CancellationToken;
use yunta_core::Criterion;

/// What this invocation knows of the measurement.
#[derive(Debug, Clone, Default)]
struct Known {
    /// Commands being measured that have not answered.
    pending: BTreeSet<String>,
    /// Commands that answered, and whether they passed.
    measured: BTreeMap<String, bool>,
}

/// The commands a run's tasks are held to, as the measurement settles them.
#[derive(Default)]
pub struct HeldGate {
    known: tokio::sync::watch::Sender<Known>,
}

impl HeldGate {
    /// `commands` are being measured.
    pub fn expect(&self, commands: &[String]) {
        self.known.send_modify(|known| {
            known
                .pending
                .extend(commands.iter().map(|command| command.trim().to_string()));
        });
    }

    /// `command` answered, passing or not.
    pub fn settle(&self, command: &str, passed: bool) {
        let command = command.trim().to_string();
        self.known.send_modify(|known| {
            known.pending.remove(&command);
            known.measured.insert(command, passed);
        });
    }

    /// The measurement stopped without answering what it still owed: a
    /// guard waiting on one of those runs the command itself, as one would
    /// with no measurement at all.
    pub fn give_up(&self) {
        self.known.send_if_modified(|known| {
            let waiting = !known.pending.is_empty();
            known.pending.clear();
            waiting
        });
    }

    /// The commands that hold a run's tasks: each its lineage `measured`
    /// green, in the order given, then each still being measured while the
    /// run goes on.
    pub fn holding(&self, measured: &[(String, bool)]) -> Vec<String> {
        let mut held: Vec<String> = measured
            .iter()
            .filter(|(_, passed)| *passed)
            .map(|(command, _)| command.trim().to_string())
            .collect();
        let known = self.known.borrow();
        for command in &known.pending {
            let answered = measured
                .iter()
                .any(|(measured, _)| measured.trim() == command);
            if !answered && !held.contains(command) {
                held.push(command.clone());
            }
        }
        held
    }

    /// Whether `criterion` is a held command still being measured.
    pub fn pending(&self, criterion: &Criterion) -> bool {
        criterion.is_guard() && self.known.borrow().pending.contains(criterion.cmd.trim())
    }

    /// `guards` split by whether they can run now, in their order: a held
    /// command still being measured waits apart, and one measured red
    /// holds nothing, so it is in neither.
    pub fn split(&self, guards: Vec<Criterion>) -> (Vec<Criterion>, Vec<Criterion>) {
        let known = self.known.borrow();
        let mut ready = Vec::new();
        let mut pending = Vec::new();
        for guard in guards {
            let cmd = guard.cmd.trim();
            if known.pending.contains(cmd) {
                pending.push(guard);
            } else if known.measured.get(cmd) != Some(&false) {
                ready.push(guard);
            }
        }
        (ready, pending)
    }

    /// `guards`, as the measurement leaves them: waiting for it while one
    /// of them is a command still being measured, and without each that
    /// was red before the run changed anything. `cancel` stops the wait,
    /// leaving every guard in.
    pub async fn judging(
        &self,
        mut guards: Vec<Criterion>,
        cancel: &CancellationToken,
    ) -> Vec<Criterion> {
        let mut known = self.known.subscribe();
        loop {
            let now = known.borrow_and_update().clone();
            if guards
                .iter()
                .any(|guard| now.pending.contains(guard.cmd.trim()))
            {
                tokio::select! {
                    changed = known.changed() => {
                        if changed.is_err() {
                            return guards;
                        }
                    }
                    () = cancel.cancelled() => return guards,
                }
                continue;
            }
            guards.retain(|guard| now.measured.get(guard.cmd.trim()) != Some(&false));
            return guards;
        }
    }
}
