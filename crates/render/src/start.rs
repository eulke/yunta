//! How a run starts, in the one line every way of starting one says: its
//! whole id, the record a script and a log keep; what is happening to it;
//! and the command that follows it by its handle. And what the control
//! plane answers a client that started or resumed one, in the same words.

use surco::words::Standing;
use yunta_core::step::NextStep;
use yunta_core::RunId;

use crate::command;

/// What a run handed to a detached process is doing, in the words every door
/// says it in.
pub(crate) const INDEPENDENTLY: &str = "driving forward independently";

/// What is happening to a run as it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// It was created, and this process drives it.
    Created,
    /// It was taken back off its log, and this process drives it again.
    Resuming,
    /// It was handed to a detached process, which drives it on its own.
    Detached,
}

impl Start {
    /// What is happening to the run, as its line says it.
    fn doing(self) -> String {
        match self {
            Start::Created => Standing::Created.word().to_string(),
            Start::Resuming => "resuming".to_string(),
            Start::Detached => format!("detached, {INDEPENDENTLY}"),
        }
    }
}

/// The line a client that started `run` reads first: its whole id, so a client
/// that reads one line still reads what it asked for.
pub fn started_id(run: &RunId) -> String {
    format!("run_id: {run}")
}

/// What a client that handed `run` back to a detached resume is answered.
pub fn resumed(run: &RunId) -> String {
    format!("run {run}: resumed, {INDEPENDENTLY}")
}

/// The line `run` starts with, as `start` starts it.
pub fn line(run: &RunId, start: Start) -> String {
    format!(
        "run {run}: {} (`{}` follows it)",
        start.doing(),
        command::line(&NextStep::Follow { run: run.clone() })
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_starts_with_its_whole_id_and_the_command_that_follows_it() {
        let run = RunId::from_static("01K3W48MFW7H0ZZA5PZ0ABCDEF");
        let follow = command::line(&NextStep::Follow { run: run.clone() });
        assert_eq!(
            line(&run, Start::Created),
            format!("run {run}: created (`{follow}` follows it)")
        );
        assert_eq!(
            line(&run, Start::Detached),
            format!("run {run}: detached, driving forward independently (`{follow}` follows it)")
        );
        assert!(line(&run, Start::Resuming).contains(": resuming ("));
    }
}
