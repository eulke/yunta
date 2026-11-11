//! See [`crate`]. What a piece of work came to, as the performer hands it back.

use std::path::PathBuf;

use yunta_api::{FieldReading, Repository, WaitingDecision, WaitingQuestions};
use yunta_core::RunId;
use yunta_render::page::{CancelState, Catalog, Node, Run};
use yunta_render::refusal::Refused;

/// What an [`Operation`](crate::Operation) came to: one variant per operation's data,
/// and three for one that did not do what it was asked.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// The repository the application works in, as far as Yunta goes.
    Repository(Repository),
    /// The workflows the repository can run.
    Catalog(Box<Catalog>),
    /// The runs read, and where the project keeps them.
    Field {
        reading: FieldReading,
        storage: PathBuf,
    },
    /// The run no longer moves, and its following ended.
    Followed,
    /// A run's page.
    Page(Box<Run>),
    /// The page of a run's node.
    NodePage(Box<Node>),
    /// The round of questions a run is parked on.
    Questions(Box<WaitingQuestions>),
    /// The decision a run waits on, with the documents it shows.
    Decision(Box<WaitingDecision>),
    /// The decision recorded, and the run handed on to a detached resume.
    Decided { run: RunId },
    /// The answers recorded, and the run handed on to a detached resume.
    Answered { run: RunId },
    /// The run started detached.
    Started { run: RunId },
    /// The run handed back to a detached resume.
    Resumed { run: RunId },
    /// How cancelling the run ended.
    Cancelled { run: RunId, state: CancelState },
    /// The runs closed, and why each of the others was not.
    Closed {
        closed: Vec<RunId>,
        refused: Vec<Refused>,
    },
    /// What Yunta refused, which render says.
    Refused(Refused),
    /// What failed below Yunta, in its own words.
    Failed(String),
    /// The work was stopped before it ended.
    Stopped,
}
