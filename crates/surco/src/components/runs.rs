//! A field of runs, one to a row.

use crate::words::Standing;

/// Runs, one to a row, in the order a reader answers them: where each
/// stands, the handle a reader types next, what it runs, the furrow its
/// nodes plow, what holds it, and — at the row's far edge — how long it has
/// stood there or the command that moves it on.
pub struct Runs {
    pub rows: Vec<RunLine>,
}

/// One run's row.
pub struct RunLine {
    pub standing: Standing,
    /// What a reader types to name the run.
    pub handle: String,
    /// The workflow it runs, and the mode it runs in.
    pub workflow: String,
    pub mode: String,
    /// Where each of its nodes stands, in the order the workflow declares
    /// them.
    pub furrow: Vec<Standing>,
    /// What holds it, in one line; empty when nothing does.
    pub said: String,
    pub edge: Edge,
}

/// What a run's row ends with, at the line's far edge.
pub enum Edge {
    /// How long the run has stood where it is.
    Age(String),
    /// The command that moves it on.
    Command(String),
}

impl Edge {
    /// What the edge says.
    pub fn text(&self) -> &str {
        match self {
            Edge::Age(age) | Edge::Command(age) => age,
        }
    }
}
