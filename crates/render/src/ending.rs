//! What a run's end says while it works, and what it says when it cannot go
//! on: a cancel signalling the engine a run's registry names, waiting on it,
//! escalating to the process groups it registered, and what a cancel or a
//! close could not stop, remove or give back.
//!
//! What a cancel or a close answers with is a page, [`crate::page`]'s
//! `Cancelled` and `Closed`; these are the lines said before it, each in the
//! order the work meets it, and the sentences of what stops the work.

use std::time::Duration;

use surco::figures::duration;
use yunta_core::step::NextStep;
use yunta_core::{Pid, RunId};

use crate::command;

/// What a cancel says as it interrupts the live engine of `run`: the
/// engine's pid.
pub fn signalling(run: &RunId, engine: Pid) -> String {
    format!(
        "run {}: signalling the live engine (pid {engine})",
        run.handle()
    )
}

/// What a cancel says when whoever asked for it stops waiting on the engine
/// it interrupted: the engine stops `run` on its own, and where to read how
/// far it got.
pub fn stopped_waiting(run: &RunId) -> String {
    format!(
        "run {}: interrupted while waiting; the engine has the SIGINT and stops on its own, \
         and `{}` says where it got to",
        run.handle(),
        command::line(&NextStep::status(run))
    )
}

/// What a cancel says when the engine of `run` it interrupted did not stop
/// within `waited`, before it kills the engine's process groups.
pub fn escalating(run: &RunId, waited: Duration) -> String {
    format!(
        "run {}: the engine did not stop within {}; escalating to SIGKILL on its process groups",
        run.handle(),
        duration(waited)
    )
}

/// What a cancel says of process group `pgid`, which a run's registry names
/// and which is no run's own: the registry is corrupt, so the group is
/// skipped.
pub fn group_refused(pgid: Pid) -> String {
    format!(
        "refusing to signal process group {pgid}: not a run's own process group, so engine.json \
         is corrupt; skipping it"
    )
}

/// What a cancel says when the registry naming a dead engine stays on disk:
/// what the disk said.
pub fn registry_kept(said: &str) -> String {
    format!("could not delete engine.json: {said}")
}

/// What a cancel or a close says of a checkout of `run` that could not go
/// back to the project's pool, and stays the run's until `gc` collects it:
/// what stopped it.
pub fn checkout_kept(run: &RunId, said: &str) -> String {
    format!("run {}: {said}", run.handle())
}

/// Why a cancel cannot signal the engine of `run`: its registry is there and
/// this binary cannot read it, as `said`; a resume recovers the run.
pub fn registry_unread(run: &RunId, said: &str) -> String {
    format!(
        "run {}: its `engine.json` is there and this binary cannot read it ({said}): `{}` \
         recovers the run once its process has stopped.",
        run.handle(),
        command::line(&NextStep::Resume { run: run.clone() })
    )
}

/// Why a cancel cannot signal the engine of `run`: a node is in progress and
/// no registry names the engine running it; a resume recovers the run.
pub fn no_registry(run: &RunId) -> String {
    format!(
        "run {} has a node in progress but no `engine.json` to signal through: the engine that \
         ran it predates this build, or its scratch directory is gone. `{}` recovers the run \
         once its process has stopped.",
        run.handle(),
        command::line(&NextStep::Resume { run: run.clone() })
    )
}

/// Why a cancel did not interrupt the live engine of `run`: what the system
/// said of the signal, and the cancel to try again.
pub fn unsignalled(run: &RunId, said: &str) -> String {
    format!(
        "{said}; retry `{}`",
        command::line(&NextStep::Cancel { run: run.clone() })
    )
}

/// What a close says when the run's close is on its log and its events
/// could not be written out beside it, as `detail` says.
pub fn log_unexported(detail: &str) -> String {
    format!("could not write the run's events.jsonl: {detail}")
}

/// What a person who interrupted a run this process drives reads while the
/// run stops: its sessions are interrupted first, then killed.
pub const STOPPING_AT_INTERRUPT: &str =
    "interrupt received: stopping the run (sessions get interrupt, then kill)";

#[cfg(test)]
mod tests {
    use super::log_unexported;

    #[test]
    fn a_close_whose_log_cannot_be_written_says_why() {
        assert_eq!(
            log_unexported("disk full"),
            "could not write the run's events.jsonl: disk full"
        );
    }
}
