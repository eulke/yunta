//! What a run's derived phase is called, and how the invocation that
//! drove it there exits.

use yunta_engine::{RunFrame, RunPhase};

use super::state::Mark;

/// What a run's derived phase is called, wherever a surface says it.
///
/// Ten words for the ten answers a reader acts on. A listing, a status
/// page, a closing block and a JSON document each used to reach into
/// [`RunPhase`] and choose a word, so one stop could be called four
/// things — and whether the command succeeded was decided a third time,
/// somewhere else again. The word is decided here; what qualifies it
/// (what a run waits on, what broke it, which mode it promoted to) stays
/// on the phase, because that is detail rather than vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunWord {
    Created,
    Running,
    /// Its log says it is moving, and the engine that drove it is gone:
    /// nothing moves it until a person resumes it.
    Stalled,
    /// Stopped on a person: a decision, a question, a budget, a scope.
    NeedsYou,
    Finished,
    /// Finished holding findings that block it: work nobody has
    /// accepted.
    Reported,
    Failed,
    Cancelled,
    Promoted,
    /// The log stopped making sense.
    Broken,
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
    /// A person interrupted the invocation, as a shell reports a process
    /// SIGINT ended.
    Interrupted = 130,
}

impl RunExit {
    pub(crate) fn outcome(self) -> crate::error::Outcome {
        crate::error::Outcome::Code(self as u8)
    }
}

impl RunWord {
    /// Every word: what sizes a column of them, and what the tests that
    /// hold the set closed walk.
    pub(crate) const ALL: [RunWord; 10] = [
        RunWord::Created,
        RunWord::Running,
        RunWord::Stalled,
        RunWord::NeedsYou,
        RunWord::Finished,
        RunWord::Reported,
        RunWord::Failed,
        RunWord::Cancelled,
        RunWord::Promoted,
        RunWord::Broken,
    ];

    /// The cells a column of run words takes: as wide as the widest word,
    /// derived from the words so a word added or renamed sizes the column
    /// with it.
    pub(crate) const WIDEST: usize = widest_run(&Self::ALL);

    /// What the run `frame` derives is called: its phase, and for a run
    /// that finished, whether findings still block it.
    pub(crate) fn of(frame: &RunFrame) -> Self {
        match &frame.phase {
            RunPhase::Created => RunWord::Created,
            RunPhase::Running => RunWord::Running,
            RunPhase::Waiting { .. } => RunWord::NeedsYou,
            RunPhase::Finished if frame.blocking_findings > 0 => RunWord::Reported,
            RunPhase::Finished => RunWord::Finished,
            RunPhase::Failed { .. } => RunWord::Failed,
            RunPhase::Cancelled => RunWord::Cancelled,
            RunPhase::Promoted { .. } => RunWord::Promoted,
            RunPhase::Broken { .. } => RunWord::Broken,
        }
    }

    /// What a run read from outside the process driving it is called:
    /// [`RunWord::of`] its frame, except that a run whose log says it is
    /// moving while the engine its registry names is gone is stalled.
    ///
    /// Only a dead engine is proof. No registry at all is also what a
    /// run looks like for the instant it is handed to another process,
    /// and calling that stalled would be wrong for exactly that instant.
    pub(crate) fn observed(frame: &RunFrame, engine: yunta_engine::EngineLiveness) -> Self {
        match (RunWord::of(frame), engine) {
            (RunWord::Created | RunWord::Running, yunta_engine::EngineLiveness::Dead) => {
                RunWord::Stalled
            }
            (word, _) => word,
        }
    }

    /// The word a person reads, and the token a document carries — one
    /// spelling, because a reader who greps a log for what `status`
    /// printed should find what `--json` published.
    pub(crate) const fn word(self) -> &'static str {
        match self {
            RunWord::Created => "created",
            RunWord::Running => "running",
            RunWord::Stalled => "stalled",
            RunWord::NeedsYou => "needs you",
            RunWord::Finished => "finished",
            RunWord::Reported => "reported",
            RunWord::Failed => "failed",
            RunWord::Cancelled => "cancelled",
            RunWord::Promoted => "promoted",
            RunWord::Broken => "broken",
        }
    }

    /// The mark that repeats the word for the eye.
    pub(crate) fn mark(self) -> Mark {
        match self {
            RunWord::Created | RunWord::Running => Mark::Running,
            RunWord::NeedsYou => Mark::NeedsYou,
            RunWord::Stalled | RunWord::Reported => Mark::Caution,
            RunWord::Finished => Mark::Done,
            RunWord::Failed | RunWord::Cancelled | RunWord::Broken => Mark::Failed,
            RunWord::Promoted => Mark::Reroute,
        }
    }

    /// How the invocation that drove a run to this word exits: 0 for a
    /// run that finished, 3 for one that stopped on a person, 4 for one
    /// that finished holding blocking findings, 1 for every other stop.
    /// One mapping, because "did the command succeed" is one question.
    pub(crate) fn exit(self) -> crate::error::Outcome {
        match self {
            RunWord::Finished => crate::error::Outcome::Success,
            RunWord::NeedsYou => RunExit::NeedsYou.outcome(),
            RunWord::Reported => RunExit::Reported.outcome(),
            RunWord::Created
            | RunWord::Running
            | RunWord::Stalled
            | RunWord::Failed
            | RunWord::Cancelled
            | RunWord::Promoted
            | RunWord::Broken => crate::error::Outcome::Reported,
        }
    }
}

const fn widest_run(words: &[RunWord]) -> usize {
    match words {
        [] => 0,
        [first, rest @ ..] => {
            let own = first.word().len();
            let others = widest_run(rest);
            if own > others {
                own
            } else {
                others
            }
        }
    }
}

impl serde::Serialize for RunWord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.word())
    }
}

impl std::fmt::Display for RunWord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.word())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::cell_width;

    #[test]
    fn every_run_word_fits_the_run_word_column() {
        for word in RunWord::ALL {
            assert!(cell_width(word.word()) <= RunWord::WIDEST, "{word:?}");
        }
        assert_eq!(RunWord::WIDEST, "needs you".len());
    }
}
