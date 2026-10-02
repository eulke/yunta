//! What a node's derived state is called, and the detail that qualifies
//! the word.
//!
//! These are the words a test case's `expect.nodes` is written in, so
//! the vocabulary a run is judged against and the vocabulary a person
//! reads are the same one, and neither can drift from the other.
//!
//! [`RunWord`] is the same thing for the run as a whole: what its
//! derived phase is called, wherever a surface says it.

use yunta_engine::{NodeStanding, NodeState, NodeWait, RunFrame, RunPhase};

/// The state a node is in, as a surface says it.
///
/// Six words for the six answers a reader acts on: it finished, it
/// failed, it is running, it waits on a person, this run leaves it out,
/// it never ran. The word carries all of that on its own — a mark beside
/// it repeats it for the eye, and color repeats it again, so a line
/// stripped of both still says the same thing. One vocabulary: the
/// column of a table and the sentence of a page say the same word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateWord {
    Finished,
    Failed,
    Running,
    Waiting,
    Skipped,
    NeverRan,
}

/// Every word, in the order a reader meets them.
pub(crate) const ALL_WORDS: [StateWord; 6] = [
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
pub(crate) const STATE_WIDTH: usize = widest(&ALL_WORDS);

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
    pub(crate) fn of(state: Option<&NodeState>) -> Self {
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
    pub(crate) const fn word(self) -> &'static str {
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
    pub(crate) fn mark(self) -> Mark {
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
pub(crate) enum Mark {
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
pub(crate) const ALL_MARKS: [Mark; 8] = [
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

    /// Where a node of a frame stands, in the vocabulary every surface
    /// says it in: the one entry a surface reading a `RunFrame` uses.
    pub(crate) fn standing(standing: &NodeStanding) -> Self {
        match standing {
            NodeStanding::Skipped => Self::skipped(),
            NodeStanding::LeftOut(because) => Self {
                word: StateWord::Skipped,
                modifier: detail(&format!("not in this run: {because}")),
            },
            NodeStanding::ToGo => Self::of(None),
            NodeStanding::Reached(state) => Self::of(Some(state)),
        }
    }

    /// A node this run's mode leaves out. It is not a node that has yet
    /// to start: this run never reaches it, and a reader who cannot tell
    /// the two apart goes looking for a node that is never coming.
    pub(crate) fn skipped() -> Self {
        Self::plain(StateWord::Skipped)
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
    /// Every word, for the tests that hold the set closed: one that
    /// proves each has a group in the listing, and one that proves each
    /// reads as itself.
    #[cfg(test)]
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
    pub(crate) fn word(self) -> &'static str {
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

/// One spelling on the wire too: a document carries the word.
/// A node's word travels as the word a reader sees, never as a variant
/// name: one vocabulary for the page and for the document.
impl serde::Serialize for StateWord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.word())
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
