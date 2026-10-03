//! What a node's derived state is called, and the detail that qualifies
//! the word.
//!
//! These are the words a test case's `expect.nodes` is written in, so
//! the vocabulary a run is judged against and the vocabulary a person
//! reads are the same one, and neither can drift from the other.
//!
//! [`RunWord`] is the same thing for the run as a whole: what its
//! derived phase is called, wherever a surface says it.

use yunta_core::events::{NodeState, NodeWait};

pub use super::run_word::RunWord;

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
    Waiting,
    Skipped,
    NeverRan,
}

/// Every word, in the order a reader meets them.
pub const ALL_WORDS: [StateWord; 6] = [
    StateWord::Finished,
    StateWord::Failed,
    StateWord::Running,
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
    /// The word `state` is called by. A node the log carries nothing for
    /// never ran: absence is a state, and the one a reader most often
    /// wants named.
    pub fn of(state: Option<&NodeState>) -> Self {
        match state {
            None => Self::NeverRan,
            Some(NodeState::Running { .. }) => Self::Running,
            Some(NodeState::Finished { .. }) => Self::Finished,
            Some(NodeState::Failed { .. }) => Self::Failed,
            Some(NodeState::Waiting { .. }) => Self::Waiting,
        }
    }

    /// The word: what `yunta status` prints, what a table's column holds
    /// and what a `yunta test` case's `expect.nodes` is written in.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Finished => "finished",
            Self::Failed => "failed",
            Self::Running => "running",
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
            Self::Waiting => Mark::NeedsYou,
            Self::Skipped => Mark::Skipped,
            Self::NeverRan => Mark::Pending,
        }
    }
}

/// What a line is marked with, before its word: a glyph the eye finds
/// first, and the color a terminal that draws one paints it.
///
/// Each mark means one thing. `NeedsYou` is for a person and nothing
/// else — a reader who learns that mark learns where they are wanted —
/// so a re-route and a caution have marks of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Done,
    Failed,
    Running,
    /// Something waits on a person.
    NeedsYou,
    Skipped,
    Pending,
    /// Control went somewhere else: a re-route, a promotion.
    Reroute,
    /// Worth a reader's attention, and nothing anybody has to answer: a
    /// run nobody drives, findings that still block, a red baseline.
    Caution,
}

/// Every mark, for a test that has to hold all of them at once.
#[cfg(test)]
pub const ALL_MARKS: [Mark; 8] = [
    Mark::Done,
    Mark::Failed,
    Mark::Running,
    Mark::NeedsYou,
    Mark::Skipped,
    Mark::Pending,
    Mark::Reroute,
    Mark::Caution,
];

/// A node's state as a surface shows it: the word that carries the
/// meaning, and the detail that qualifies it.
pub struct NodeDisplay {
    pub word: StateWord,
    /// The outcome a node finished with, why it failed, which attempt is
    /// running, the handle it waits on — already collapsed onto one line
    /// through [`yunta_core::text::one_line`], because every surface that
    /// shows it has room for exactly one. `None` when the word says all
    /// there is.
    pub modifier: Option<String>,
}

impl NodeDisplay {
    /// How `state` reads: the word [`StateWord::of`] gives it, and what
    /// that state carries beyond the word.
    pub fn of(state: Option<&NodeState>) -> Self {
        Self {
            word: StateWord::of(state),
            modifier: match state {
                None => None,
                Some(NodeState::Running { attempt }) => Some(format!("attempt {attempt}")),
                Some(NodeState::Finished { outcome, .. }) => detail(outcome),
                // The claim, not the lines a command printed: those are
                // evidence, one step away on every surface that has room.
                Some(NodeState::Failed { failure, .. }) => detail(&failure.headline()),
                Some(NodeState::Waiting { on }) => match on {
                    NodeWait::Gate { external_ref } => external_ref.as_deref().and_then(detail),
                    // The one sentence for a node that asked: the same
                    // bytes the chronicle and the run's pause use.
                    NodeWait::Questions { asked } => {
                        detail(&yunta_core::text::asked_questions(asked.as_slice()))
                    }
                },
            },
        }
    }

    /// A node this run leaves out, `because` saying why.
    pub fn left_out(because: &str) -> Self {
        Self {
            word: StateWord::Skipped,
            modifier: detail(&format!("not in this run: {because}")),
        }
    }

    /// A node this run's mode leaves out. It is not a node that has yet
    /// to start: this run never reaches it, and a reader who cannot tell
    /// the two apart goes looking for a node that is never coming.
    pub fn skipped() -> Self {
        Self::plain(StateWord::Skipped)
    }

    /// One line: the word, then what qualifies it after a dash.
    pub fn label(&self) -> String {
        match &self.modifier {
            Some(detail) => format!("{} — {detail}", self.word.word()),
            None => self.word.word().to_string(),
        }
    }

    fn plain(word: StateWord) -> Self {
        Self {
            word,
            modifier: None,
        }
    }
}

/// `text` on one line, or `None` when it says nothing — an empty
/// modifier would leave a line ending in a dash that introduces nothing.
fn detail(text: &str) -> Option<String> {
    let text = yunta_core::text::one_line(text);
    (!text.is_empty()).then_some(text)
}

/// One spelling on the wire too: a document carries the word.
/// A node's word travels as the word a reader sees, never as a variant
/// name: one vocabulary for the page and for the document.
impl serde::Serialize for StateWord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.word())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell_width;
    use yunta_core::events::{Failure, TokenUsage};

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
    fn a_finished_node_reads_as_its_outcome_on_one_line() {
        let state = NodeState::Finished {
            outcome: "exit 0\nwith a second line".to_string(),
            tokens: TokenUsage::default(),
        };
        let display = NodeDisplay::of(Some(&state));
        assert_eq!(display.word, StateWord::Finished);
        assert_eq!(display.label(), "finished — exit 0 with a second line");
    }

    #[test]
    fn a_failed_node_reads_as_its_failure() {
        let state = NodeState::Failed {
            failure: Failure::Message {
                outcome: "exit 1".to_string(),
            },
            tokens: TokenUsage::default(),
            retryable: false,
        };
        assert_eq!(NodeDisplay::of(Some(&state)).label(), "failed — exit 1");
    }

    #[test]
    fn a_state_with_nothing_to_qualify_it_is_the_word_alone() {
        assert_eq!(NodeDisplay::of(None).label(), "never ran");
        assert_eq!(NodeDisplay::skipped().label(), "skipped");
        let waiting = NodeState::Waiting {
            on: NodeWait::Gate { external_ref: None },
        };
        assert_eq!(NodeDisplay::of(Some(&waiting)).label(), "waiting");
    }

    #[test]
    fn a_waiting_node_reads_as_the_handle_it_waits_on() {
        let waiting = NodeState::Waiting {
            on: NodeWait::Gate {
                external_ref: Some("https://forge/pr/7".to_string()),
            },
        };
        assert_eq!(
            NodeDisplay::of(Some(&waiting)).label(),
            "waiting — https://forge/pr/7"
        );
    }

    /// A node that asked says what it asked, in the one sentence every
    /// surface says it with.
    #[test]
    fn a_waiting_node_that_asked_names_its_questions() {
        let waiting = NodeState::Waiting {
            on: NodeWait::Questions {
                asked: yunta_core::NonEmpty::new(vec![
                    yunta_core::QuestionId::from("q-scope"),
                    yunta_core::QuestionId::from("q-api"),
                ])
                .expect("a node that asked, asked something"),
            },
        };
        assert_eq!(
            NodeDisplay::of(Some(&waiting)).label(),
            "waiting — asked 2 questions: `q-scope`, `q-api`"
        );
    }
}
