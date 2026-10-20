//! What a node's derived state is called, and the detail that qualifies
//! the word.
//!
//! The words themselves are Surco's ([`surco::words`]); this module says
//! which of them a node's state from the log is called by, and what
//! qualifies it.

use surco::words::StateWord;
use yunta_core::events::{NodeState, NodeWait};

/// The word `state` is called by. A node the log carries nothing for
/// never ran: absence is a state, and the one a reader most often
/// wants named.
pub fn word_of(state: Option<&NodeState>) -> StateWord {
    match state {
        None => StateWord::NeverRan,
        Some(NodeState::Running { .. }) => StateWord::Running,
        Some(NodeState::Finished { .. }) => StateWord::Finished,
        Some(NodeState::Failed { .. }) => StateWord::Failed,
        Some(NodeState::Waiting { .. }) => StateWord::Waiting,
    }
}

/// A node's state as a surface shows it: the word that carries the
/// meaning, and the detail that qualifies it.
pub struct NodeDisplay {
    pub word: StateWord,
    /// The outcome a node finished with, why it failed, which attempt is
    /// running, the handle it waits on — already collapsed onto one line
    /// through [`surco::text::one_line`], because every surface that
    /// shows it has room for exactly one. `None` when the word says all
    /// there is.
    pub modifier: Option<String>,
}

impl NodeDisplay {
    /// How `state` reads: the word [`word_of`] gives it, and what
    /// that state carries beyond the word.
    pub fn of(state: Option<&NodeState>) -> Self {
        Self {
            word: word_of(state),
            modifier: match state {
                None => None,
                Some(NodeState::Running { attempt }) => Some(format!("attempt {attempt}")),
                Some(NodeState::Finished { outcome, .. }) => {
                    detail(&crate::outcome::sentence(outcome))
                }
                // The claim, not the lines a command printed: those are
                // evidence, one step away on every surface that has room.
                Some(NodeState::Failed { failure, .. }) => {
                    detail(&crate::failure::headline(failure))
                }
                Some(NodeState::Waiting { on }) => match on {
                    NodeWait::Gate { external_ref } => external_ref.as_deref().and_then(detail),
                    // The one sentence for a node that asked: the same
                    // bytes the chronicle and the run's pause use.
                    NodeWait::Questions { asked } => {
                        detail(&crate::pause::asked_questions(asked.as_slice()))
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

    /// A node whose attempt `attempt` was open when its run paused.
    pub fn paused(attempt: u32) -> Self {
        Self {
            word: StateWord::Paused,
            modifier: Some(format!("attempt {attempt}")),
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
    let text = surco::text::one_line(text);
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::events::{Failure, NodeOutcome, TokenUsage};

    #[test]
    fn a_finished_node_reads_as_its_outcome_on_one_line() {
        let state = NodeState::Finished {
            outcome: NodeOutcome::Summary("exit 0\nwith a second line".to_string()),
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
                outcome: "exit 1".into(),
            },
            tokens: TokenUsage::default(),
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
