//! The words the engine's own types are called by, and how the
//! invocation that drove a run exits for the word it ended on.

use chrono::{DateTime, Utc};
use yunta_engine::{EngineLiveness, NodeStanding, Prompt, RunFrame, RunPhase, WaitingOn};
use yunta_render::{NodeDisplay, RunWord};

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

/// What the run `frame` derives is called: its phase, and for a run that
/// finished, whether findings still block it.
pub(crate) fn run_word(frame: &RunFrame) -> RunWord {
    match &frame.phase {
        RunPhase::Created => RunWord::Created,
        RunPhase::Running => RunWord::Running,
        // A limit's reset resumes the run on its own, so the wait is not a stop a person answers.
        RunPhase::Waiting {
            on: WaitingOn::Run { reason },
        } if reason.reset_wait().is_some() => RunWord::Waiting,
        RunPhase::Waiting { .. } => RunWord::NeedsYou,
        RunPhase::Finished if frame.blocking_findings > 0 => RunWord::Reported,
        RunPhase::Finished => RunWord::Finished,
        RunPhase::Failed { .. } => RunWord::Failed,
        RunPhase::Cancelled => RunWord::Cancelled,
        RunPhase::Promoted { .. } => RunWord::Promoted,
        RunPhase::Broken { .. } => RunWord::Broken,
    }
}

/// What a run's registry says about the process driving it, read at one
/// instant: whether it lives, and whom it is asking at its terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Engine {
    pub(crate) liveness: EngineLiveness,
    pub(crate) prompt: Option<Prompt>,
    /// When it was read, which is what a question has been waiting from.
    pub(crate) at: DateTime<Utc>,
}

impl Engine {
    /// What the registry of the run under `run_dir` says at `at`.
    pub(crate) fn of(run_dir: &std::path::Path, at: DateTime<Utc>) -> Self {
        let probe = yunta_engine::lock::SystemProbe;
        Engine {
            liveness: yunta_engine::engine_liveness(run_dir, &probe),
            prompt: yunta_engine::engine_prompt(run_dir, &probe),
            at,
        }
    }

    /// This invocation, driving the run itself: alive, and asking no one
    /// a reader elsewhere could answer.
    pub(crate) fn this(at: DateTime<Utc>) -> Self {
        Engine {
            liveness: EngineLiveness::Alive,
            prompt: None,
            at,
        }
    }
}

/// What a run read from outside the process driving it is called:
/// [`run_word`] of its frame, except that a run whose log says it is
/// moving while the engine its registry names is gone is stalled, and
/// one whose engine is asking a person at its terminal needs them.
///
/// Only a dead engine is proof. No registry at all is also what a run
/// looks like for the instant it is handed to another process, and
/// calling that stalled would be wrong for exactly that instant.
pub(crate) fn observed_word(frame: &RunFrame, engine: &Engine) -> RunWord {
    match run_word(frame) {
        RunWord::Created | RunWord::Running if engine.liveness == EngineLiveness::Dead => {
            RunWord::Stalled
        }
        RunWord::Created | RunWord::Running if engine.prompt.is_some() => RunWord::NeedsYou,
        word => word,
    }
}

/// How the invocation that drove a run to `word` exits: 0 for a run that
/// finished, 3 for one that stopped on a person, 4 for one that finished
/// holding blocking findings, 5 for one parked until a limit resets, 1 for
/// every other stop. One mapping, because "did the command succeed" is one
/// question.
pub(crate) fn exit(word: RunWord) -> Outcome {
    match word {
        RunWord::Finished => Outcome::Success,
        RunWord::NeedsYou => RunExit::NeedsYou.outcome(),
        RunWord::Reported => RunExit::Reported.outcome(),
        RunWord::Waiting => RunExit::Waiting.outcome(),
        RunWord::Created
        | RunWord::Running
        | RunWord::Stalled
        | RunWord::Failed
        | RunWord::Cancelled
        | RunWord::Promoted
        | RunWord::Broken => Outcome::Reported,
    }
}

/// Where a node of a frame stands, in the vocabulary every surface says
/// it in: the one entry a surface reading a `RunFrame` uses.
pub(crate) fn standing(standing: &NodeStanding) -> NodeDisplay {
    match standing {
        NodeStanding::Skipped => NodeDisplay::skipped(),
        NodeStanding::LeftOut(because) => NodeDisplay::left_out(&because.to_string()),
        NodeStanding::ToGo => NodeDisplay::of(None),
        NodeStanding::Paused { attempt } => NodeDisplay::paused(*attempt),
        NodeStanding::Reached(state) => NodeDisplay::of(Some(state)),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use yunta_core::events::PauseReason;
    use yunta_core::{NodeId, RunId};

    use super::*;

    /// The frame of a run paused on `reason`, the rest of it as the test
    /// kit draws a run.
    fn paused_on(reason: PauseReason) -> RunFrame {
        RunFrame {
            phase: RunPhase::Waiting {
                on: WaitingOn::Run { reason },
            },
            ..yunta_testkit::run_frame(&RunId::from_static("run-1"))
        }
    }

    #[test]
    fn a_run_paused_until_a_limit_resets_reads_waiting_and_exits_on_its_own() {
        let at = chrono::Utc
            .with_ymd_and_hms(2026, 10, 13, 12, 0, 0)
            .unwrap();
        let frame = paused_on(PauseReason::AwaitingReset {
            node: NodeId::from_static("build"),
            at,
        });
        assert_eq!(run_word(&frame), RunWord::Waiting);
        assert_eq!(run_word(&frame).word(), "waiting");
        assert_eq!(exit(run_word(&frame)), Outcome::Code(5));
    }

    #[test]
    fn a_run_paused_on_a_person_still_reads_needs_you() {
        let frame = paused_on(PauseReason::BudgetExhausted { spent: 5, cap: 4 });
        assert_eq!(run_word(&frame), RunWord::NeedsYou);
        assert_eq!(exit(run_word(&frame)), Outcome::Code(3));
    }
}
