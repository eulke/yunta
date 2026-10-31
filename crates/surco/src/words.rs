//! Where a run or a node stands, as every surface says it.
//!
//! One vocabulary for both, because a reader meets them side by side: the
//! word a listing calls a run, the word its row calls a node, the word its
//! JSON carries and the word a `yunta test` case is written in are one
//! word. A test case judged in these words and a page read in them cannot
//! drift apart.

use crate::glyphs::Mark;

/// Where a run or a node stands, in the one vocabulary every surface says
/// it in.
///
/// Each word is an answer a reader acts on, and the mark beside it repeats
/// it for the eye: a line stripped of mark and color still says the same
/// thing. `needs you` is for a person and nothing else — a run waiting on a
/// clock is `waiting`, and one a person stopped is `stopped`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Standing {
    /// A run nothing ran in yet.
    Created,
    Running,
    /// A clock alone: a limit resets, and the run resumes by itself.
    Waiting,
    /// A person owes an answer: a decision, a question, a budget, a scope.
    NeedsYou,
    /// Its log says it is moving, and no engine drives it: nothing moves it
    /// until a person resumes it.
    Stalled,
    Finished,
    /// Finished holding findings that block it: work nobody has accepted.
    Reported,
    /// Ended in error, with no route and no decision waiting.
    Failed,
    /// A node that failed and that a re-route is correcting.
    Rerouted,
    /// A person stopped the run, and a resume continues it; a node whose
    /// attempt stopped with its run.
    Stopped,
    /// A run that closed for a successor in a later mode.
    Promoted,
    /// The log stopped making sense.
    Broken,
    /// A node this run leaves out.
    Skipped,
    /// A node nothing ran yet.
    NeverRan,
}

impl Standing {
    /// Every word, in the order a reader meets them.
    pub const ALL: [Standing; 14] = [
        Standing::Created,
        Standing::Running,
        Standing::Waiting,
        Standing::NeedsYou,
        Standing::Stalled,
        Standing::Finished,
        Standing::Reported,
        Standing::Failed,
        Standing::Rerouted,
        Standing::Stopped,
        Standing::Promoted,
        Standing::Broken,
        Standing::Skipped,
        Standing::NeverRan,
    ];

    /// The cells a column of these words takes: as wide as the widest word,
    /// derived from the words so a word added or renamed sizes the column
    /// with it.
    pub const WIDEST: usize = widest(&Self::ALL);

    /// The word a person reads, and the token a document carries — one
    /// spelling, because a reader who greps a log for what `status`
    /// printed should find what `--json` published.
    pub const fn word(self) -> &'static str {
        match self {
            Standing::Created => "created",
            Standing::Running => "running",
            Standing::Waiting => "waiting",
            Standing::NeedsYou => "needs you",
            Standing::Stalled => "stalled",
            Standing::Finished => "finished",
            Standing::Reported => "reported",
            Standing::Failed => "failed",
            Standing::Rerouted => "rerouted",
            Standing::Stopped => "stopped",
            Standing::Promoted => "promoted",
            Standing::Broken => "broken",
            Standing::Skipped => "skipped",
            Standing::NeverRan => "never ran",
        }
    }

    /// The mark that repeats the word for the eye.
    pub const fn mark(self) -> Mark {
        match self {
            Standing::Created | Standing::Running | Standing::Waiting => Mark::Running,
            Standing::NeedsYou => Mark::NeedsYou,
            Standing::Stalled | Standing::Reported => Mark::Caution,
            Standing::Finished => Mark::Done,
            Standing::Failed | Standing::Broken => Mark::Failed,
            Standing::Rerouted | Standing::Promoted => Mark::Reroute,
            Standing::Stopped => Mark::Stopped,
            Standing::Skipped => Mark::Skipped,
            Standing::NeverRan => Mark::Absent,
        }
    }

    /// The standing `word` names, read back: what a `yunta test` case wrote,
    /// in the words a surface prints.
    pub fn named(word: &str) -> Option<Standing> {
        Self::ALL
            .into_iter()
            .find(|standing| standing.word() == word)
    }
}

/// What a page says when nothing needs the person.
pub const NOTHING_NEEDS_YOU: &str = "nothing needs you";

/// The plural of `needs you`, for a count of runs that need a person.
pub const NEED_YOU: &str = "need you";

const fn widest(words: &[Standing]) -> usize {
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

/// One spelling on the wire too: a document carries the word a reader
/// sees, never a variant name.
impl serde::Serialize for Standing {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.word())
    }
}

impl std::fmt::Display for Standing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.word())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::width::cell_width;

    #[test]
    fn every_word_fits_the_column_of_words() {
        for standing in Standing::ALL {
            assert!(
                cell_width(standing.word()) <= Standing::WIDEST,
                "{standing:?}"
            );
        }
        assert_eq!(Standing::WIDEST, "never ran".len());
    }

    #[test]
    fn every_word_is_spelled_once_and_reads_back() {
        for standing in Standing::ALL {
            assert_eq!(Standing::named(standing.word()), Some(standing));
        }
        assert_eq!(Standing::named("paused"), None);
        assert_eq!(Standing::named("cancelled"), None);
    }

    #[test]
    fn only_a_person_owed_an_answer_takes_the_person_mark() {
        let person: Vec<Standing> = Standing::ALL
            .into_iter()
            .filter(|standing| standing.mark() == Mark::NeedsYou)
            .collect();
        assert_eq!(person, vec![Standing::NeedsYou]);
    }

    #[test]
    fn a_wait_on_a_clock_moves_on_its_own_and_a_stop_is_neutral() {
        assert_eq!(Standing::Waiting.mark(), Mark::Running);
        assert_eq!(Standing::Stopped.mark(), Mark::Stopped);
        assert_ne!(Standing::Stopped.mark(), Standing::Skipped.mark());
    }
}
