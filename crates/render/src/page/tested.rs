//! What `yunta test` says as its cases run: each case's row as it ends, in
//! the column every case's name lines up in, and the tally under them; and
//! how one case's run ended, which the pages about a pack say of its own
//! cases too. Running a case is its caller's.

use std::path::PathBuf;

use surco::components::{Check, Checklist, Found, Headline};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use surco::text::counted;
use surco::words::Standing;
use yunta_core::events::{PauseReason, TaskStatus};
use yunta_core::step::NextStep;
use yunta_core::NodeId;

use super::Page;
use crate::command;

/// How one case's run ended.
#[derive(Debug, Clone, PartialEq)]
pub enum CaseRun {
    /// It reached everything its `expect` says.
    Held,
    /// It ran, and fell short of its `expect` in each of these.
    Failed(Vec<CaseProblem>),
    /// It could not run, said in the words of what stopped it.
    CouldNotRun(String),
}

/// Something a case's run did not hold of what the case expects, each said
/// as a line a person compares with the case.
#[derive(Debug, Clone, PartialEq)]
pub enum CaseProblem {
    /// The fixture at `fixture` describes sessions the run never opened, by
    /// their place in it, from 1: a fixture is the case's own account of
    /// the run, so one nobody made fails the case, always.
    Unclaimed {
        fixture: PathBuf,
        sessions: Vec<usize>,
    },
    /// The run ended on `got`, not the `expected` word, stopped by
    /// `stopped_by` when a pause or a failed node ended it.
    FinalState {
        expected: Standing,
        got: Standing,
        stopped_by: Option<PauseReason>,
    },
    /// The case expects `node` at `expected`, and the workflow declares no
    /// such node.
    NoSuchNode { node: String, expected: String },
    /// The case expects `node` at `expected`, and it stands at `got`.
    Node {
        node: NodeId,
        expected: String,
        got: Standing,
    },
    /// The case expects `task` at `expected`, and it stands at `got`, or
    /// never registered.
    Task {
        task: String,
        expected: String,
        got: Option<TaskStatus>,
    },
}

impl CaseProblem {
    /// The problem as a person reads it under the case's row.
    pub fn said(&self) -> String {
        match self {
            CaseProblem::Unclaimed { fixture, sessions } => format!(
                "fixture `{}`: {} nothing opened: {}",
                fixture.display(),
                counted(sessions.len(), "scripted session"),
                sessions
                    .iter()
                    .map(|place| format!("#{place}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            CaseProblem::FinalState {
                expected,
                got,
                stopped_by,
            } => match stopped_by {
                Some(reason) => format!(
                    "final_state: expected {expected}, got {got}\n    {}",
                    surco::text::hanging(&crate::pause::sentence(reason), "    ")
                ),
                None => format!("final_state: expected {expected}, got {got}"),
            },
            CaseProblem::NoSuchNode { node, expected } => {
                format!("node {node}: expected {expected}, and the workflow declares no such node")
            }
            CaseProblem::Node {
                node,
                expected,
                got,
            } => format!(
                "node {node}: expected {expected}, got {got}{}",
                renamed(expected, *got)
            ),
            CaseProblem::Task {
                task,
                expected,
                got,
            } => format!(
                "task {task}: expected {expected}, got {}",
                got.map_or("never registered", TaskStatus::as_str)
            ),
        }
    }
}

/// Why a node expected in a word the vocabulary moved stands at another:
/// a node waiting on a person is `needs you` now, and `waiting` is a
/// clock's.
fn renamed(expected: &str, got: Standing) -> String {
    let said = match (expected, got) {
        ("waiting", Standing::NeedsYou) => Some(format!(
            "a node waiting on a person is `{}` now; `waiting` is a clock's",
            Standing::NeedsYou.word()
        )),
        _ => retired(expected),
    };
    said.map(|said| format!(" ({said})")).unwrap_or_default()
}

/// Why `word` is no word a case's run can end on, of `ends`: the word that
/// replaced it when the vocabulary retired it, or every word it can be.
pub fn not_a_final_state(word: &str, ends: &[Standing]) -> String {
    retired(word).unwrap_or_else(|| {
        format!(
            "`{word}` is not a final state; one of: {}",
            ends.iter()
                .map(|end| end.word())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

/// What a case is told for a word the vocabulary retired: the word that
/// replaced it, and what it says.
fn retired(word: &str) -> Option<String> {
    match word {
        "paused" => Some(format!(
            "`paused` is called `{}` now when a person owes an answer, and `{}` when a person \
             stopped it",
            Standing::NeedsYou.word(),
            Standing::Stopped.word()
        )),
        "cancelled" => Some(format!(
            "`cancelled` is called `{}` now: a person stopped it",
            Standing::Stopped.word()
        )),
        _ => None,
    }
}

impl CaseRun {
    /// Whether it held.
    pub fn held(&self) -> bool {
        matches!(self, CaseRun::Held)
    }

    /// What its row finds.
    fn found(&self) -> Found {
        match self {
            CaseRun::Held => Found::Holds,
            CaseRun::Failed(_) | CaseRun::CouldNotRun(_) => Found::Problem,
        }
    }

    /// What its row says: that it held, or how many problems it had and
    /// whether it ran.
    fn said(&self) -> String {
        match self {
            CaseRun::Held => "ok".to_string(),
            CaseRun::Failed(problems) => format!("failed: {}", counted(problems.len(), "error")),
            CaseRun::CouldNotRun(_) => "could not run: 1 error".to_string(),
        }
    }

    /// Each problem, as written: what fell short, or why it could not run.
    fn problems(&self) -> Vec<String> {
        match self {
            CaseRun::Held => Vec::new(),
            CaseRun::Failed(problems) => problems.iter().map(CaseProblem::said).collect(),
            CaseRun::CouldNotRun(error) => vec![error.clone()],
        }
    }
}

/// A case, by its name, and how its run ended.
#[derive(Debug, Clone, PartialEq)]
pub struct RanCase {
    pub name: String,
    pub run: CaseRun,
}

impl RanCase {
    /// Its row: what it found, by its name, with each problem under it.
    pub(super) fn check(&self) -> Check {
        Check::new(self.run.found(), self.name.as_str(), self.run.said())
            .details(self.run.problems())
    }
}

/// A case tested, said as it ends: what it found, in the column every case's
/// name lines up in, with what went wrong under it.
pub struct TestedCase<'a> {
    /// Every case's name, which sets the column the rows share.
    pub names: &'a [String],
    pub case: RanCase,
}

impl Page for TestedCase<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let mut row = Checklist::aligned(self.names.to_vec());
        row.push_check(self.case.check());
        Doc::new().with(row)
    }
}

/// The verdict under the rows it counts, as a run's closing comes under
/// what it showed while it ran: how many cases ran, and how many failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tested {
    pub cases: usize,
    pub failures: usize,
}

impl Page for Tested {
    fn document(&self, _look: &Look) -> Doc {
        let tally = format!(
            "{}, {} failed",
            surco::text::counted(self.cases, "case"),
            self.failures
        );
        let mark = match self.failures {
            0 => Mark::Done,
            _ => Mark::Failed,
        };
        Doc::new().with(Headline::new(
            mark,
            command::line(&NextStep::Test { dir: None }),
            tally,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixture_nobody_opened_names_each_unclaimed_session_by_its_place() {
        let said = CaseProblem::Unclaimed {
            fixture: PathBuf::from("fixtures/review.yaml"),
            sessions: vec![2, 3],
        }
        .said();
        assert_eq!(
            said,
            "fixture `fixtures/review.yaml`: 2 scripted sessions nothing opened: #2, #3"
        );
    }

    #[test]
    fn a_final_state_stopped_by_a_pause_says_the_pause_under_the_word() {
        let stopped = CaseProblem::FinalState {
            expected: Standing::Finished,
            got: Standing::Stopped,
            stopped_by: Some(PauseReason::Cancelled),
        };
        let finished = CaseProblem::FinalState {
            expected: Standing::Failed,
            got: Standing::Finished,
            stopped_by: None,
        };
        assert_eq!(
            stopped.said(),
            format!(
                "final_state: expected finished, got stopped\n    {}",
                crate::pause::sentence(&PauseReason::Cancelled)
            )
        );
        assert_eq!(
            finished.said(),
            "final_state: expected failed, got finished"
        );
    }

    #[test]
    fn a_node_in_a_moved_word_is_told_the_word_it_moved_to() {
        let waiting = CaseProblem::Node {
            node: NodeId::from_static("review"),
            expected: "waiting".to_string(),
            got: Standing::NeedsYou,
        };
        let paused = CaseProblem::Node {
            node: NodeId::from_static("review"),
            expected: "paused".to_string(),
            got: Standing::Stopped,
        };
        assert_eq!(
            waiting.said(),
            "node review: expected waiting, got needs you (a node waiting on a person is \
             `needs you` now; `waiting` is a clock's)"
        );
        assert!(
            paused
                .said()
                .starts_with("node review: expected paused, got stopped (`paused` is called"),
            "{}",
            paused.said()
        );
    }

    #[test]
    fn a_node_or_a_task_the_run_lacks_says_so() {
        let node = CaseProblem::NoSuchNode {
            node: "lint".to_string(),
            expected: "finished".to_string(),
        };
        let task = CaseProblem::Task {
            task: "t1".to_string(),
            expected: "done".to_string(),
            got: None,
        };
        assert_eq!(
            node.said(),
            "node lint: expected finished, and the workflow declares no such node"
        );
        assert_eq!(task.said(), "task t1: expected done, got never registered");
    }

    #[test]
    fn a_word_that_is_no_final_state_names_the_ones_there_are() {
        let ends = [Standing::Finished, Standing::Failed];
        assert_eq!(
            not_a_final_state("done", &ends),
            "`done` is not a final state; one of: finished, failed"
        );
        assert!(not_a_final_state("cancelled", &ends).contains("`stopped`"));
    }
}
