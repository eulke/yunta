//! See [`crate`]. What a stage asks of Yunta, as data, which the performer carries out
//! through `yunta-api`.
//!
//! One variant per thing a stage asks, named by its verb; each stage that asks
//! something new adds its variant here, and the performer carries it out.

use yunta_api::{FieldReach, RunRequest};
use yunta_core::{Answer, NodeId, OptionId, RunId};

/// Work a stage asks Yunta to do, named by its verb. Not a command line a person
/// types ([`yunta_core::step::NextStep`]): this is what the application asks the
/// performer, and an answer comes back as an [`Outcome`](crate::Outcome).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// The repository the application works in, as far as Yunta goes: none, one never
    /// set up, or one set up.
    ReadRepository,
    /// The workflows the repository can run.
    ReadCatalog,
    /// The runs as far as `reach` reaches.
    Field { reach: FieldReach },
    /// `run` followed while it moves, onto the successor a promotion hands the work to.
    Follow { run: RunId },
    /// `run`'s page.
    ReadPage { run: RunId },
    /// The page of `run`'s node `node`.
    ReadNodePage { run: RunId, node: NodeId },
    /// The round of questions `run` is parked on.
    ReadQuestions { run: RunId },
    /// The decision `run` waits on, with the documents it shows.
    ReadDecision { run: RunId },
    /// `option` recorded on the decision `run` waits on, with the `words` it was chosen
    /// with, and the run handed on to a detached resume.
    Decide {
        run: RunId,
        option: OptionId,
        words: Option<String>,
    },
    /// `answers` to the round `node` asked recorded on `run`, and the run handed on to
    /// a detached resume.
    Answer {
        run: RunId,
        node: NodeId,
        answers: Vec<Answer>,
    },
    /// Hands `run` back to a detached resume.
    Resume { run: RunId },
    /// Cancels `run` and everything under it.
    Cancel { run: RunId },
    /// Closes each of `runs` for good.
    Close { runs: Vec<RunId> },
    /// The run `request` asks for, admitted, created and started detached.
    Start { request: RunRequest },
}

impl Operation {
    /// Whether the operation only reads, so one asked again supersedes one of its kind
    /// still under way, whose answer would be stale by the time it came.
    pub(crate) fn reads(&self) -> bool {
        match self {
            Operation::ReadRepository
            | Operation::ReadCatalog
            | Operation::Field { .. }
            | Operation::Follow { .. }
            | Operation::ReadPage { .. }
            | Operation::ReadNodePage { .. }
            | Operation::ReadQuestions { .. }
            | Operation::ReadDecision { .. } => true,
            Operation::Resume { .. }
            | Operation::Cancel { .. }
            | Operation::Close { .. }
            | Operation::Decide { .. }
            | Operation::Answer { .. }
            | Operation::Start { .. } => false,
        }
    }
}
