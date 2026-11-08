//! What a run's registry says about the engine driving it, as values.
//! Reading `engine.json` asks the host's process table, so it stays in the
//! engine crate, which returns an [`Engine`]; a reading of a run takes it as
//! an input, the way it takes the instant it is read at.
//!
//! The registry names one process, the engine, and what it is doing: the
//! question it holds at its terminal, the socket it serves the run's words
//! on. So the pid is the engine's, on [`Engine::Alive`], and a [`Prompt`] is
//! the question alone, the very record `engine.json` holds. A sentence about
//! an engine that asks takes an [`Asking`], which joins the two and measures
//! how long it has asked.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use yunta_core::{NodeId, Pid};

/// What a run's registry says about the engine driving it, as far as the
/// registry can prove. Not the engine itself: a reading of what it wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Engine {
    /// The process the registry names is the one that wrote it.
    Alive {
        /// The engine's process: whose terminal a question is asked at.
        pid: Pid,
        /// The question it holds at its terminal, while it asks one.
        prompt: Option<Prompt>,
        /// The socket it serves the run's words on, while it serves one.
        words: Option<PathBuf>,
    },
    /// The registry names a process that is gone, or a number the host has
    /// since given to another process: nothing drives the run.
    Dead,
    /// No registry. No process is driving the run now — one deletes it on
    /// every way out, a pause included — or one is about to write it: a run
    /// handed to a detached process has none for an instant.
    Unrecorded,
    /// A registry this binary cannot read, or a process the host cannot tell
    /// about.
    Unknown,
}

impl Engine {
    /// The process reading the run is the one driving it, `pid`: alive,
    /// asking no one a reader elsewhere could answer, and naming no socket,
    /// since what it hears it hears in its own process. Replay reads no
    /// process, as it reads no clock, so the caller hands it its own pid.
    pub fn this(pid: Pid) -> Self {
        Engine::Alive {
            pid,
            prompt: None,
            words: None,
        }
    }

    /// The question the live process holds at its terminal; `None` when it
    /// asks no one, or none lives.
    pub fn prompt(&self) -> Option<&Prompt> {
        match self {
            Engine::Alive { prompt, .. } => prompt.as_ref(),
            Engine::Dead | Engine::Unrecorded | Engine::Unknown => None,
        }
    }

    /// The socket the live process serves the run's words on; `None` when it
    /// serves none, or none lives.
    pub fn words(&self) -> Option<&Path> {
        match self {
            Engine::Alive { words, .. } => words.as_deref(),
            Engine::Dead | Engine::Unrecorded | Engine::Unknown => None,
        }
    }

    /// Whether the process driving the run is asking a person about `node`
    /// at its terminal.
    pub fn asks_about(&self, node: &NodeId) -> bool {
        self.prompt().is_some_and(|prompt| prompt.is_about(node))
    }

    /// Whom the live process is asking at its terminal, measured at `at`:
    /// the one place an [`Asking`] is built. `None` when it asks no one, or
    /// none lives.
    pub fn asking(&self, at: DateTime<Utc>) -> Option<Asking> {
        let Engine::Alive {
            pid,
            prompt: Some(prompt),
            ..
        } = self
        else {
            return None;
        };
        Some(Asking {
            pid: *pid,
            prompt: prompt.clone(),
            // A prompt stamped after the instant it is read at has not
            // waited yet: the host's clock and the registry's disagree.
            waited: (at - prompt.since).to_std().unwrap_or_default(),
        })
    }
}

/// The question a live process holds at its terminal: the node it asks
/// about, when one does, and since when. The record `engine.json` holds,
/// under these field names, so the file and its reading are one type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prompt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
    pub since: DateTime<Utc>,
}

impl Prompt {
    /// Whether the question is about `node`; one the run asks itself is
    /// about none.
    pub fn is_about(&self, node: &NodeId) -> bool {
        self.node.as_ref() == Some(node)
    }
}

/// The process driving a run asking a person at its terminal, measured at an
/// instant: whose terminal, the question, and how long it had been asked by
/// then. Every sentence about a process that asks says one of these, at the
/// length its place has room for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asking {
    pub pid: Pid,
    pub prompt: Prompt,
    pub waited: Duration,
}
