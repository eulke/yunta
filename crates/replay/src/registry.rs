//! What a run's registry says about the engine driving it, as values.
//! Reading `engine.json` asks the host's process table, so it stays in the
//! engine, which returns these; a reading of a run takes them as inputs.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use yunta_core::{NodeId, Pid};

/// Whether a live engine is driving a run, as far as the run's own
/// registry can prove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineLiveness {
    /// The process the registry names is the one that wrote it.
    Alive,
    /// The registry names a process that is gone, or a number the host
    /// has since given to another process: nothing drives the run.
    Dead,
    /// No registry. No engine is driving the run now — one deletes it on
    /// every way out, a pause included — or one is about to write it: a
    /// run handed to a detached process has none for an instant.
    Unrecorded,
    /// A registry this binary cannot read, or a process the host cannot
    /// tell about.
    Unknown,
}

/// A person a live engine is asking at its terminal: the node that asks,
/// since when, and the process whose terminal holds the question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub node: Option<NodeId>,
    pub since: DateTime<Utc>,
    pub pid: Pid,
}

/// The engine driving a run, as the run's registry says at an instant:
/// whether it lives, whom it is asking at its terminal, and when the
/// registry was read.
///
/// A frame takes it the way it takes `now` — an input its caller read —
/// so the word a run and a node are called by is decided with it, once,
/// and never corrected after by the surface that draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    pub liveness: EngineLiveness,
    pub prompt: Option<Prompt>,
    /// The socket the engine serves the run's words on, while it lives and
    /// serves one.
    pub words: Option<PathBuf>,
    /// When the registry was read: what a question has been waiting from.
    pub at: DateTime<Utc>,
}

impl Engine {
    /// The process reading the run is the engine driving it: alive, asking
    /// no one a reader elsewhere could answer, and naming no socket, since
    /// what it hears it hears in its own process.
    pub fn this(at: DateTime<Utc>) -> Self {
        Engine {
            liveness: EngineLiveness::Alive,
            prompt: None,
            words: None,
            at,
        }
    }

    /// Whether the engine driving the run is asking a person about `node`
    /// at its terminal.
    pub fn asks_about(&self, node: &NodeId) -> bool {
        self.prompt.as_ref().and_then(|prompt| prompt.node.as_ref()) == Some(node)
    }
}
