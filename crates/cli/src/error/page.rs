//! A refusal as the page a person reads: the headline sentence of a
//! refusal that found problems in a text or several runs a reference could
//! mean, and the blocks under it. An agent reads no page: the same problems
//! reach it as plain text, in the refusal's sentence.

use surco::components::{Problem, RunLine, Runs, Said};
use surco::doc::Block;

use super::CliError;
use crate::commands::run_ref::Candidate;
use crate::commands::verdict::Unrunnable;

/// What a refused workflow is refused for, said before its problems.
const UNRUNNABLE: &str = "the workflow fails `yunta check`";

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
            CliError::Unrunnable(unrunnable) => {
                Some(counted_under(UNRUNNABLE, unrunnable.problems.len()))
            }
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
            CliError::Unrunnable(unrunnable) => unrunnable.blocks(Vec::new()),
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

/// Every problem of a refused document, each quoted from its text.
pub(super) fn document_problems(
    report: &yunta_core::diagnostic::Report,
    text: Option<&str>,
) -> Vec<Problem> {
    yunta_render::blocks::diagnostic::report(report, text)
}

/// A workflow that cannot run here, as plain text says it: its problems
/// counted and quoted, then what to declare.
pub(super) fn unrunnable(unrunnable: &Unrunnable) -> String {
    use surco::width::INDENT;
    let mut said = yunta_render::blocks::diagnostic::plain(UNRUNNABLE, unrunnable.problems.clone());
    for fix in &unrunnable.declare {
        said.push_str(&format!("\n{INDENT}{}", fix.said));
        for line in &fix.paste {
            said.push_str(&format!("\n{INDENT}{INDENT}{INDENT}{line}"));
        }
    }
    said
}
