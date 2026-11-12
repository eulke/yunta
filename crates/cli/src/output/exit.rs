//! The code a process exits with: what a command came back as
//! ([`Outcome`]), and the code the word a run ended on maps to ([`exit`]),
//! which a script that started the run reads instead of parsing its output.

use surco::words::Standing;

use crate::shell::EndSignal;

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

/// The code a shell reports for a process SIGINT ended: 128 plus the
/// signal's number, which a person's Ctrl-C reads as.
const INTERRUPTED: u8 = 130;

/// The code a shell reports for a process SIGTERM ended: 128 plus the
/// signal's number.
const TERMINATED: u8 = 143;

/// The code a shell reports for a process SIGHUP ended: 128 plus the
/// signal's number.
const HUNG_UP: u8 = 129;

impl Outcome {
    /// How an invocation a person interrupted exits, as a shell reports a
    /// process SIGINT ended.
    pub(crate) fn interrupted() -> Self {
        Outcome::Code(INTERRUPTED)
    }

    /// How an invocation `signal` asked to end exits, as a shell reports a
    /// process that signal ended.
    pub(crate) fn ended_by(signal: EndSignal) -> Self {
        match signal {
            EndSignal::Interrupt => Outcome::interrupted(),
            EndSignal::Terminate => Outcome::Code(TERMINATED),
            EndSignal::HangUp => Outcome::Code(HUNG_UP),
        }
    }
}

/// How the invocation that drove a run to `standing` exits: the code the
/// run's word maps to, which every document of the run carries
/// ([`yunta_render::document::exit_code`]), as the outcome it is.
pub(crate) fn exit(standing: Standing) -> Outcome {
    match yunta_render::document::exit_code(standing) {
        0 => Outcome::Success,
        1 => Outcome::Reported,
        code => Outcome::Code(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signal_that_asks_to_end_exits_as_a_shell_reports_it() {
        assert_eq!(Outcome::ended_by(EndSignal::Interrupt).code(), 130);
        assert_eq!(Outcome::ended_by(EndSignal::Terminate).code(), 143);
        assert_eq!(Outcome::ended_by(EndSignal::HangUp).code(), 129);
    }

    #[test]
    fn a_run_waiting_on_a_clock_exits_on_its_own_and_a_stop_is_not_a_success() {
        assert_eq!(exit(Standing::Waiting), Outcome::Code(5));
        assert_eq!(exit(Standing::NeedsYou), Outcome::Code(3));
        assert_eq!(exit(Standing::Reported), Outcome::Code(4));
        assert_eq!(exit(Standing::Finished), Outcome::Success);
        assert_eq!(exit(Standing::Stopped), Outcome::Reported);
    }
}
