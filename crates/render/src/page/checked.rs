//! The verdict on a workflow file, as a person reads it — whether it can run
//! here, what was found, and the check to run again — and what `check`
//! diagnosed, which a page says beneath it: why the workflow cannot run, each
//! problem quoted from its file, and what does not stop it but should be
//! heard. Reaching the verdict is its caller's.

use std::path::{Path, PathBuf};

use surco::components::{Fields, Furrow, Headline, Next, Problem};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::look::Look;
use surco::text::counted;
use yunta_core::step::{Arg, NextStep};

use super::Page;

/// Why a workflow cannot run here: every problem, each pointed at in its
/// line when it has one, and what this project could declare for the ones
/// a declaration fixes.
#[derive(Debug)]
pub struct Unrunnable {
    /// The workflow refused, as a reader in this directory names its file.
    pub workflow: String,
    pub problems: Vec<Problem>,
    pub declare: Vec<surco::components::Fix>,
}

impl Unrunnable {
    /// The blocks a page draws: each problem, then `warnings`, then what to
    /// declare.
    pub fn blocks(&self, warnings: &[Problem]) -> Vec<Block> {
        self.problems
            .iter()
            .chain(warnings)
            .cloned()
            .map(Block::from)
            .chain(self.declare.iter().flat_map(surco::components::Fix::blocks))
            .collect()
    }
}

/// What `check` diagnosed: why the workflow cannot run here, each problem
/// quoted from its file, and what does not stop it but should be heard. Not
/// surco's `Found`, the result of one check's row.
pub struct Diagnosis {
    pub unrunnable: Unrunnable,
    /// Each warning, as the problem a page says it in.
    pub warnings: Vec<Problem>,
}

impl Diagnosis {
    /// How many problems stop the workflow.
    pub fn errors(&self) -> usize {
        self.unrunnable.problems.len()
    }

    /// The verdict in words: the workflow can run here, or how many
    /// problems stop it.
    pub fn said(&self) -> String {
        match self.errors() {
            0 => "can run here".to_string(),
            errors => counted(errors, "error"),
        }
    }

    /// What a page draws of it: each problem, each warning, and what this
    /// project could declare.
    pub fn blocks(&self) -> Vec<Block> {
        self.unrunnable.blocks(&self.warnings)
    }
}

/// The verdict on a workflow file, as `check` prints it and `graph` beside
/// its drawing: whether it can run here, what was found, and the check to
/// run again once it is fixed.
pub struct Checked {
    /// The workflow file the verdict is on.
    pub path: PathBuf,
    /// Where it is, as a reader in this directory names it.
    pub at: String,
    pub diagnosis: Diagnosis,
}

impl Page for Checked {
    fn document(&self, _look: &Look) -> Doc {
        let diagnosis = &self.diagnosis;
        let mark = match diagnosis.errors() {
            0 => Mark::Done,
            _ => Mark::Failed,
        };
        let mut headline = Headline::new(mark, file_named(&self.path), diagnosis.said());
        if !diagnosis.warnings.is_empty() {
            headline = headline.fact(counted(diagnosis.warnings.len(), "warning"));
        }
        let steps = match diagnosis.errors() {
            0 => Vec::new(),
            _ => vec![check_again(&self.at)],
        };
        Doc::new()
            .with(headline)
            .with(Furrow(Vec::new()))
            .with(Fields::new().push_if("at", self.at.as_str()))
            .with_all(diagnosis.blocks())
            .with(Next { steps })
    }
}

/// A workflow file as a headline names it: its name, the folder it is in
/// being what the lines its problems point at say.
pub(super) fn file_named(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The check to run once what a verdict found in the file `shown` names
/// is fixed.
pub(super) fn check_again(shown: &str) -> (String, &'static str) {
    (
        crate::command::line(&NextStep::Check {
            workflow: Arg::Given(shown.to_string()),
        }),
        "again, once they are fixed",
    )
}
