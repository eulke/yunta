//! A field of runs, one to a row.

use crate::words::Standing;

/// Runs, one to a row, in the order a reader answers them: where each
/// stands, the handle a reader types next, what it runs, the furrow its
/// nodes plow, what holds it, and — at the row's far edge — how long it has
/// stood there or the command that moves it on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runs {
    pub rows: Vec<RunLine>,
}

/// One run's row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunLine {
    pub standing: Standing,
    /// What a reader types to name the run.
    pub handle: String,
    /// The workflow it runs: what a printed page names beside the mode,
    /// where no glance at the run pointed at names it.
    pub workflow: String,
    /// The mode it runs in: all a screen's row names, the glance beside the
    /// list naming the workflow.
    pub mode: String,
    /// Where each of its nodes stands, in the order the workflow declares
    /// them: drawn in a column of a fixed width that starts at the same cell
    /// on every row, however many nodes a run has, so the furrows of a field
    /// read down one column.
    pub furrow: Vec<Standing>,
    /// What holds it, in one line; empty when nothing does.
    pub said: String,
    pub edge: Edge,
    /// Whether the row is the one a key would act on.
    pub pointed: bool,
    /// Whether a person picked the run for an act that takes several.
    pub marked: bool,
}

impl RunLine {
    /// What it runs, as a page names it: the workflow and, in brackets, its
    /// mode.
    pub fn name(&self) -> String {
        format!("{} ({})", self.workflow, self.mode)
    }
}

/// What a run's row ends with, at the line's far edge.
#[derive(Debug, Clone, PartialEq, Eq)]
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
