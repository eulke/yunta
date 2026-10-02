//! A document an escalation shows, read from the run for the person
//! deciding: what the log names, where its view sits, and what it says.

use crate::events::findings::RunFindings;
use crate::events::{AcceptedDeparture, Shown};
use crate::{FindingsFile, SpecFile, TasksFile};

/// One document an escalation shows, as the run holds it: what the log
/// names, where its view sits, and what it says.
#[derive(Debug, Clone, PartialEq)]
pub struct ShownDocument {
    pub shown: Shown,
    pub path: std::path::PathBuf,
    pub content: ShownContent,
}

/// A shown document's content: a tasks document read into its tasks, a
/// spec into its specs, a findings document into its findings, any other
/// as its text.
#[derive(Debug, Clone, PartialEq)]
pub enum ShownContent {
    /// A plan, with every departure from it a person accepted while its
    /// tasks were built — where the work stops being what the plan says.
    Tasks {
        plan: TasksFile,
        departed: Vec<AcceptedDeparture>,
    },
    /// The tests a plan's tasks are held to, read into its specs.
    Spec(SpecFile),
    /// What a review found, read into its findings.
    Findings(FindingsFile),
    /// Every finding standing in the run, each with the node that
    /// reported it and how other nodes answered it.
    RunFindings(RunFindings),
    Text(String),
}
