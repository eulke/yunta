//! What a node's derived state is called, and the detail that qualifies
//! the word.
//!
//! These are the words a test case's `expect.nodes` is written in, so
//! the vocabulary a run is judged against and the vocabulary a person
//! reads are the same one, and neither can drift from the other.
//!
//! [`RunWord`] is the same thing for the run as a whole: what its
//! derived phase is called, wherever a surface says it.

use yunta_engine::{NodeState, RunPhase};

/// The cells the short word gets in a column of them. Four, the width of
/// the longest of the six.
pub(crate) const STATE_WIDTH: usize = 4;

/// The state a node is in, as a surface says it.
///
/// Six words for the six answers a reader acts on: it is done, it
/// failed, it is running, it waits on a person, this run leaves it out,
/// it has not started. The word carries all of that on its own — a glyph
/// beside it repeats it for the eye, and color repeats it again, so a
/// line stripped of both still says the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateWord {
    Done,
    Fail,
    Run,
    Wait,
    Skip,
    Todo,
}

impl StateWord {
    /// The word `state` is called by. A node the log carries nothing for
    /// never ran: absence is a state, and the one a reader most often
    /// wants named.
    pub(crate) fn of(state: Option<&NodeState>) -> Self {
        match state {
            None => Self::Todo,
            Some(NodeState::Running { .. }) => Self::Run,
            Some(NodeState::Finished { .. }) => Self::Done,
            Some(NodeState::Failed { .. }) => Self::Fail,
            Some(NodeState::Waiting { .. }) => Self::Wait,
        }
    }

    /// The word for a fixed-width column, [`STATE_WIDTH`] cells or
    /// narrower, so a column of them stays a column.
    pub(crate) fn short(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Fail => "fail",
            Self::Run => "run",
            Self::Wait => "wait",
            Self::Skip => "skip",
            Self::Todo => "todo",
        }
    }

    /// The word for a line with room for a sentence: what `yunta status`
    /// prints and what a `yunta test` case's `expect.nodes` is written
    /// in.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Done => "finished",
            Self::Fail => "failed",
            Self::Run => "running",
            Self::Wait => "waiting",
            Self::Skip => "skipped",
            Self::Todo => "never ran",
        }
    }
}

/// Every word, for a test that has to hold all six at once.
#[cfg(test)]
pub(crate) const ALL_WORDS: [StateWord; 6] = [
    StateWord::Done,
    StateWord::Fail,
    StateWord::Run,
    StateWord::Wait,
    StateWord::Skip,
    StateWord::Todo,
];

/// A node's state as a surface shows it: the word that carries the
/// meaning, and the detail that qualifies it.
pub(crate) struct NodeDisplay {
    pub(crate) word: StateWord,
    /// The outcome a node finished with, why it failed, which attempt is
    /// running, the handle it waits on — already collapsed onto one line
    /// through [`yunta_core::text::one_line`], because every surface that
    /// shows it has room for exactly one. `None` when the word says all
    /// there is.
    pub(crate) modifier: Option<String>,
}

impl NodeDisplay {
    /// How `state` reads: the word [`StateWord::of`] gives it, and what
    /// that state carries beyond the word.
    pub(crate) fn of(state: Option<&NodeState>) -> Self {
        Self {
            word: StateWord::of(state),
            modifier: match state {
                None => None,
                Some(NodeState::Running { attempt }) => Some(format!("attempt {attempt}")),
                Some(NodeState::Finished { outcome, .. }) => detail(outcome),
                Some(NodeState::Failed { failure, .. }) => detail(&failure.to_string()),
                Some(NodeState::Waiting { external_ref }) => {
                    external_ref.as_deref().and_then(detail)
                }
            },
        }
    }

    /// A node this run's mode leaves out. It is not a node that has yet
    /// to start: this run never reaches it, and a reader who cannot tell
    /// the two apart goes looking for a node that is never coming.
    pub(crate) fn skipped() -> Self {
        Self::plain(StateWord::Skip)
    }

    /// One line: the word, then what qualifies it after a dash.
    pub(crate) fn label(&self) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::cell_width;
    use yunta_core::events::{Failure, TokenUsage};

    #[test]
    fn every_short_word_fits_the_column_it_is_written_for() {
        for word in ALL_WORDS {
            assert!(cell_width(word.short()) <= STATE_WIDTH, "{word:?}");
        }
    }

    #[test]
    fn a_finished_node_reads_as_its_outcome_on_one_line() {
        let state = NodeState::Finished {
            outcome: "exit 0\nwith a second line".to_string(),
            tokens: TokenUsage::default(),
        };
        let display = NodeDisplay::of(Some(&state));
        assert_eq!(display.word, StateWord::Done);
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
        let waiting = NodeState::Waiting { external_ref: None };
        assert_eq!(NodeDisplay::of(Some(&waiting)).label(), "waiting");
    }

    #[test]
    fn a_waiting_node_reads_as_the_handle_it_waits_on() {
        let waiting = NodeState::Waiting {
            external_ref: Some("https://forge/pr/7".to_string()),
        };
        assert_eq!(
            NodeDisplay::of(Some(&waiting)).label(),
            "waiting — https://forge/pr/7"
        );
    }
}

/// What a run's derived phase is called, wherever a surface says it.
///
/// Eight words for the eight answers a reader acts on. A listing, a
/// status page, a closing block and a JSON document each used to reach
/// into [`RunPhase`] and choose a word, so one stop could be called
/// four things — and whether the command succeeded was decided a third
/// time, somewhere else again. The word is decided here; what qualifies
/// it (what a run waits on, what broke it, which mode it promoted to)
/// stays on the phase, because that is detail rather than vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunWord {
    Created,
    Running,
    /// Stopped until a person acts.
    Paused,
    Finished,
    Failed,
    Cancelled,
    Promoted,
    /// The log stopped making sense.
    Broken,
}

impl RunWord {
    /// Every word, for the tests that hold the set closed: one that
    /// proves each has a group in the listing, and one that proves each
    /// reads as itself.
    #[cfg(test)]
    pub(crate) const ALL: [RunWord; 8] = [
        RunWord::Created,
        RunWord::Running,
        RunWord::Paused,
        RunWord::Finished,
        RunWord::Failed,
        RunWord::Cancelled,
        RunWord::Promoted,
        RunWord::Broken,
    ];

    /// What `phase` is called.
    pub(crate) fn of(phase: &RunPhase) -> Self {
        match phase {
            RunPhase::Created => RunWord::Created,
            RunPhase::Running => RunWord::Running,
            RunPhase::Waiting { .. } => RunWord::Paused,
            RunPhase::Finished => RunWord::Finished,
            RunPhase::Failed { .. } => RunWord::Failed,
            RunPhase::Cancelled => RunWord::Cancelled,
            RunPhase::Promoted { .. } => RunWord::Promoted,
            RunPhase::Broken { .. } => RunWord::Broken,
        }
    }

    /// What a run this invocation drove reached.
    ///
    /// The other way one arrives at the same word: a run in flight
    /// reports a terminal rather than a derived phase, and the two are
    /// the same vocabulary — so `yunta run` and `yunta status` call one
    /// stop the same thing.
    pub(crate) fn of_terminal(terminal: &yunta_engine::RunTerminal) -> Self {
        match terminal {
            yunta_engine::RunTerminal::Finished => RunWord::Finished,
            yunta_engine::RunTerminal::Paused { .. } => RunWord::Paused,
            yunta_engine::RunTerminal::Failed { .. } => RunWord::Failed,
            yunta_engine::RunTerminal::Promoted { .. } => RunWord::Promoted,
        }
    }

    /// The word a person reads, and the token a document carries — one
    /// spelling, because a reader who greps a log for what `status`
    /// printed should find what `--json` published.
    pub(crate) fn word(self) -> &'static str {
        match self {
            RunWord::Created => "created",
            RunWord::Running => "running",
            RunWord::Paused => "paused",
            RunWord::Finished => "finished",
            RunWord::Failed => "failed",
            RunWord::Cancelled => "cancelled",
            RunWord::Promoted => "promoted",
            RunWord::Broken => "broken",
        }
    }

    /// The mark that repeats the word for the eye, in the same
    /// vocabulary a node's state uses.
    pub(crate) fn mark(self) -> StateWord {
        match self {
            RunWord::Created | RunWord::Running => StateWord::Run,
            RunWord::Paused | RunWord::Promoted => StateWord::Wait,
            RunWord::Finished => StateWord::Done,
            RunWord::Failed | RunWord::Cancelled | RunWord::Broken => StateWord::Fail,
        }
    }
}

/// One spelling on the wire too: a document carries the word.
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

/// Whether the invocation that reached this word reports success.
///
/// One mapping, because "did the command succeed" is one question. Only
/// a run that finished says yes: every other word is a stop that needs a
/// decision, and the block above it already says which.
impl From<RunWord> for crate::error::Outcome {
    fn from(word: RunWord) -> Self {
        match word {
            RunWord::Finished => crate::error::Outcome::Success,
            RunWord::Created
            | RunWord::Running
            | RunWord::Paused
            | RunWord::Failed
            | RunWord::Cancelled
            | RunWord::Promoted
            | RunWord::Broken => crate::error::Outcome::Reported,
        }
    }
}
