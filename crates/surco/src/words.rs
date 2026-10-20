//! What a node's state and a run's phase are called.
//!
//! These are the words a test case's `expect.nodes` is written in, so
//! the vocabulary a run is judged against and the vocabulary a person
//! reads are the same one, and neither can drift from the other.
//!
//! [`RunWord`] is the same thing for the run as a whole: what its
//! derived phase is called, wherever a surface says it.

use crate::glyphs::Mark;

/// The state a node is in, as a surface says it.
///
/// Six words for the six answers a reader acts on: it finished, it
/// failed, it is running, it waits on a person, this run leaves it out,
/// it never ran. The word carries all of that on its own — a mark beside
/// it repeats it for the eye, and color repeats it again, so a line
/// stripped of both still says the same thing. One vocabulary: the
/// column of a table and the sentence of a page say the same word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateWord {
    Finished,
    Failed,
    Running,
    /// Its attempt is open while its run is paused: nothing runs it, and
    /// the run's resume starts it again.
    Paused,
    Waiting,
    Skipped,
    NeverRan,
}

/// Every word, in the order a reader meets them.
pub const ALL_WORDS: [StateWord; 7] = [
    StateWord::Finished,
    StateWord::Failed,
    StateWord::Running,
    StateWord::Paused,
    StateWord::Waiting,
    StateWord::Skipped,
    StateWord::NeverRan,
];

/// The cells a column of state words takes: as wide as the widest word,
/// so a column of them stays a column. Derived from the words, so a word
/// added or renamed sizes the column with it.
pub const STATE_WIDTH: usize = widest(&ALL_WORDS);

const fn widest(words: &[StateWord]) -> usize {
    match words {
        [] => 0,
        [first, rest @ ..] => {
            let own = first.word().len();
            let others = widest(rest);
            if own > others {
                own
            } else {
                others
            }
        }
    }
}

impl StateWord {
    /// The word: what `yunta status` prints, what a table's column holds
    /// and what a `yunta test` case's `expect.nodes` is written in.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Finished => "finished",
            Self::Failed => "failed",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Waiting => "waiting",
            Self::Skipped => "skipped",
            Self::NeverRan => "never ran",
        }
    }

    /// The mark that repeats the word for the eye.
    pub fn mark(self) -> Mark {
        match self {
            Self::Finished => Mark::Done,
            Self::Failed => Mark::Failed,
            Self::Running => Mark::Running,
            Self::Paused => Mark::Pending,
            Self::Waiting => Mark::NeedsYou,
            Self::Skipped => Mark::Skipped,
            Self::NeverRan => Mark::Pending,
        }
    }
}

/// One spelling on the wire too: a document carries the word.
/// A node's word travels as the word a reader sees, never as a variant
/// name: one vocabulary for the page and for the document.
impl serde::Serialize for StateWord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.word())
    }
}

/// What a run's derived phase is called, wherever a surface says it.
///
/// Eleven words for the eleven answers a reader acts on. A listing, a status
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
    /// Parked until a limit resets; it resumes on its own.
    Waiting,
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
    pub const ALL: [RunWord; 11] = [
        RunWord::Created,
        RunWord::Running,
        RunWord::Waiting,
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
            RunWord::Waiting => "waiting",
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
            RunWord::Created | RunWord::Running | RunWord::Waiting => Mark::Running,
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
    use crate::width::cell_width;

    #[test]
    fn every_state_word_fits_the_state_column() {
        for word in ALL_WORDS {
            assert!(cell_width(word.word()) <= STATE_WIDTH, "{word:?}");
        }
        assert_eq!(STATE_WIDTH, "never ran".len());
    }

    #[test]
    fn only_a_person_waiting_is_drawn_with_the_person_mark() {
        let person: Vec<StateWord> = ALL_WORDS
            .into_iter()
            .filter(|word| word.mark() == Mark::NeedsYou)
            .collect();
        assert_eq!(person, vec![StateWord::Waiting]);
        let runs: Vec<RunWord> = RunWord::ALL
            .into_iter()
            .filter(|word| word.mark() == Mark::NeedsYou)
            .collect();
        assert_eq!(runs, vec![RunWord::NeedsYou]);
    }

    #[test]
    fn every_run_word_fits_the_run_word_column() {
        for word in RunWord::ALL {
            assert!(cell_width(word.word()) <= RunWord::WIDEST, "{word:?}");
        }
        assert_eq!(RunWord::WIDEST, "needs you".len());
    }

    #[test]
    fn a_run_waiting_on_a_reset_is_drawn_as_one_that_moves_on_its_own() {
        assert_eq!(RunWord::Waiting.word(), "waiting");
        assert_eq!(RunWord::Waiting.mark(), Mark::Running);
    }
}
