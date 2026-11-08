//! The code a process exits with: what a command came back as
//! ([`Outcome`]), and the code the word a run ended on maps to ([`exit`]),
//! which a script that started the run reads instead of parsing its output.

use surco::words::Standing;

/// How a subcommand came back when nothing stopped it from running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The command did its job; exit 0.
    Success,
    /// The command ran and reports a failing verdict — the detail is
    /// already on its output. Exit non-zero, no error banner.
    Reported,
    /// The command's whole answer is its exit code, because the process
    /// that ran it reads one: the rail hook, whose calling CLI takes
    /// `2` as a refusal and `0` as consent.
    Code(u8),
}

impl Outcome {
    /// The code the process exits with: 0 for a job done, 1 for a failing
    /// verdict, and a code the outcome names as itself.
    pub(crate) fn code(self) -> u8 {
        match self {
            Outcome::Success => 0,
            Outcome::Reported => 1,
            Outcome::Code(code) => code,
        }
    }
}

/// The exit codes a run's word maps to beyond success and failure — what
/// a script that started the run reads instead of parsing its output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum RunExit {
    /// The run stopped on a person.
    NeedsYou = 3,
    /// The run finished holding blocking findings.
    Reported = 4,
    /// The run paused until a limit resets and resumes on its own, so there
    /// is nothing for a person to do.
    Waiting = 5,
    /// A person interrupted the invocation, as a shell reports a process
    /// SIGINT ended.
    Interrupted = 130,
}

impl RunExit {
    pub(crate) fn outcome(self) -> Outcome {
        Outcome::Code(self as u8)
    }
}

/// How the invocation that drove a run to `standing` exits: 0 for a run
/// that finished, 3 for one that stopped on a person, 4 for one that
/// finished holding blocking findings, 5 for one parked until a limit
/// resets, 1 for every other stop. One mapping, because "did the command
/// succeed" is one question.
pub(crate) fn exit(standing: Standing) -> Outcome {
    match standing {
        Standing::Finished => Outcome::Success,
        Standing::NeedsYou => RunExit::NeedsYou.outcome(),
        Standing::Reported => RunExit::Reported.outcome(),
        Standing::Waiting => RunExit::Waiting.outcome(),
        Standing::Created
        | Standing::Running
        | Standing::Stalled
        | Standing::Failed
        | Standing::Rerouted
        | Standing::Stopped
        | Standing::Promoted
        | Standing::Broken
        | Standing::Skipped
        | Standing::NeverRan => Outcome::Reported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_waiting_on_a_clock_exits_on_its_own_and_a_stop_is_not_a_success() {
        assert_eq!(exit(Standing::Waiting), Outcome::Code(5));
        assert_eq!(exit(Standing::NeedsYou), Outcome::Code(3));
        assert_eq!(exit(Standing::Reported), Outcome::Code(4));
        assert_eq!(exit(Standing::Finished), Outcome::Success);
        assert_eq!(exit(Standing::Stopped), Outcome::Reported);
    }
}
