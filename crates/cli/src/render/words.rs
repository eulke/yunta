//! What the engine's own types read as at this terminal, and how the
//! invocation that drove a run exits for the word it ended on.

use chrono::{DateTime, Utc};
use surco::words::Standing;
use yunta_engine::{Engine, NodeFrame, NodeStanding};
use yunta_render::state::NodeDisplay;

use crate::error::Outcome;

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

/// What the registry of the run under `run_dir` says at `at` about the
/// engine driving it, as this host's processes tell.
pub(crate) fn engine(run_dir: &std::path::Path, at: DateTime<Utc>) -> Engine {
    Engine::read(run_dir, &yunta_engine::lock::SystemProbe, at)
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

/// How a node of a frame reads: the word the frame decided for it, and
/// what qualifies it.
pub(crate) fn node_display(node: &NodeFrame) -> NodeDisplay {
    match &node.state {
        NodeStanding::Skipped => NodeDisplay::skipped(),
        NodeStanding::LeftOut(because) => NodeDisplay::left_out(&because.to_string()),
        NodeStanding::ToGo => NodeDisplay::of(node.standing, None),
        NodeStanding::Paused { attempt } => NodeDisplay::held(node.standing, *attempt),
        NodeStanding::Reached(state) => NodeDisplay::of(node.standing, Some(state)),
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
