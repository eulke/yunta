//! What a run's derived phase is called.

use super::state::Mark;

/// What a run's derived phase is called, wherever a surface says it.
///
/// Ten words for the ten answers a reader acts on. A listing, a status
/// page, a closing block and a JSON document each used to reach into
/// the engine's `RunPhase` and choose a word, so one stop could be called four
/// things — and whether the command succeeded was decided a third time,
/// somewhere else again. The word is decided here; what qualifies it
/// (what a run waits on, what broke it, which mode it promoted to) stays
/// on the phase, because that is detail rather than vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunWord {
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

impl RunWord {
    /// Every word: what sizes a column of them, and what the tests that
    /// hold the set closed walk.
    pub const ALL: [RunWord; 10] = [
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
    pub const WIDEST: usize = widest_run(&Self::ALL);

    /// The word a person reads, and the token a document carries — one
    /// spelling, because a reader who greps a log for what `status`
    /// printed should find what `--json` published.
    pub const fn word(self) -> &'static str {
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
    pub fn mark(self) -> Mark {
        match self {
            RunWord::Created | RunWord::Running => Mark::Running,
            RunWord::NeedsYou => Mark::NeedsYou,
            RunWord::Stalled | RunWord::Reported => Mark::Caution,
            RunWord::Finished => Mark::Done,
            RunWord::Failed | RunWord::Cancelled | RunWord::Broken => Mark::Failed,
            RunWord::Promoted => Mark::Reroute,
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
    use crate::cell_width;

    #[test]
    fn every_run_word_fits_the_run_word_column() {
        for word in RunWord::ALL {
            assert!(cell_width(word.word()) <= RunWord::WIDEST, "{word:?}");
        }
        assert_eq!(RunWord::WIDEST, "needs you".len());
    }
}
