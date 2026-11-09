//! A refusal as the page a person reads: the headline sentence of a
//! refusal that found problems in a text or several runs a reference could
//! mean, and the blocks under it. An agent reads no page: the same problems
//! reach it as plain text, each with its `help:` line.

use surco::components::{Next, Problem, Refusal, RunLine, Runs, Said};
use surco::doc::{Block, Doc};
use surco::look::Look;
use yunta_core::diagnostic::Report;
use yunta_core::step::{Arg, NextStep};
use yunta_core::yaml::Location;
use yunta_engine::CheckError;
use yunta_render::blocks::diagnostic::{hinted, Pointed};
use yunta_render::command::Reader;
use yunta_render::page::Page;

use super::CliError;
use crate::commands::run_ref::Candidate;
use yunta_render::page::Unrunnable;

/// What a refused workflow is refused for, said before its problems: the
/// check to run on it again, on its file when the refusal knows one.
pub(crate) fn unrunnable(workflow: Arg<String>) -> String {
    format!(
        "the workflow fails `{}`",
        yunta_render::command::line(&NextStep::Check { workflow })
    )
}

impl CliError {
    /// The sentence a person's page opens with when the problems or the
    /// runs it names are blocks under it: how many problems there are, or
    /// that a run has to be named. `None` for any other refusal.
    pub(super) fn headline_for_a_person(&self) -> Option<String> {
        use yunta_render::blocks::diagnostic::counted_under;
        match self {
            CliError::Document { report, .. } => Some(counted_under(
                &report.document.path,
                report.diagnostics.len(),
            )),
            CliError::Unrunnable(refusal) => Some(counted_under(
                &unrunnable(Arg::Given(refusal.workflow.clone())),
                refusal.problems.len(),
            )),
            // The runs it could be are rows of the page, under it.
            CliError::AmbiguousRun { said, .. } => Some(name_one(said)),
            _ => None,
        }
    }

    /// The blocks a person's page draws under the refusal: each problem it
    /// found in a text, pointed at in its line, and what to declare for the
    /// ones a declaration fixes; or the runs a reference could mean. Empty
    /// for a refusal its sentence says whole.
    pub fn blocks(&self) -> Vec<Block> {
        if let CliError::AmbiguousRun { candidates, .. } = self {
            return choice(candidates);
        }
        match self {
            CliError::Document { report, text } => document_problems(report, text.as_deref())
                .into_iter()
                .map(Block::from)
                .collect(),
            CliError::Unrunnable(refusal) => refusal.blocks(&[]),
            _ => Vec::new(),
        }
    }
}

/// A sentence that several runs answer to, asking for one of them.
pub(super) fn name_one(said: &str) -> String {
    format!("{said}; name one")
}

/// The runs a reference could mean, as the rows a person chooses among:
/// each as the field shows it, and one that does not read back by its id.
fn choice(candidates: &[Candidate]) -> Vec<Block> {
    let rows: Vec<RunLine> = candidates
        .iter()
        .filter_map(|candidate| candidate.line.clone())
        .collect();
    let unread: Vec<String> = candidates
        .iter()
        .filter(|candidate| candidate.line.is_none())
        .map(|candidate| candidate.run_id.to_string())
        .collect();
    let rows = (!rows.is_empty()).then(|| Runs { rows }.into());
    let unread = (!unread.is_empty()).then(|| Said(unread).into());
    rows.into_iter().chain(unread).collect()
}

/// Every problem of a refused document, each quoted from its text, in the
/// order the report has them.
pub(crate) fn document_problems(report: &Report, text: Option<&str>) -> Vec<Problem> {
    hinted(
        report
            .diagnostics
            .iter()
            .map(|diagnostic| Pointed::found(diagnostic, &report.document.path, text))
            .collect(),
    )
}

/// One refusal of `yunta check`, quoted from `text` read from `path` at
/// `at` when it has a place, with the name it is most likely a slip from
/// said under it.
pub(crate) fn refused(
    error: &CheckError,
    at: Option<Location>,
    path: &str,
    text: Option<&str>,
) -> Problem {
    yunta_render::blocks::diagnostic::located(super::words::check_said(error), at, path, text)
        .help(error.near().map(surco::text::suggested))
}

/// A workflow that cannot run here, as plain text says it: its problems
/// counted and quoted, then what to declare.
pub(super) fn unrunnable_said(refusal: &Unrunnable) -> String {
    use surco::width::INDENT;
    let mut said = yunta_render::blocks::diagnostic::plain(
        &unrunnable(Arg::Given(refusal.workflow.clone())),
        refusal.problems.clone(),
    );
    for fix in &refusal.declare {
        said.push_str(&format!("\n{INDENT}{}", fix.said));
        for line in &fix.paste {
            said.push_str(&format!("\n{INDENT}{INDENT}{INDENT}{line}"));
        }
    }
    said
}

/// A refusal as a person reads it on stderr: the sentence and where it
/// looked, each problem it found pointed at in its line, and the commands
/// that go on from here.
impl Page for CliError {
    fn document(&self, _look: &Look) -> Doc {
        let (detail, steps) = self.advice(Reader::Person);
        Doc::new()
            .with(Refusal {
                said: self.said_to(Reader::Person),
                detail,
            })
            .with_all(self.blocks())
            .with(Next { steps })
    }
}
