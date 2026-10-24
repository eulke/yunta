//! A refusal as the page a person reads: the headline sentence of a
//! refusal that found problems in a text or several runs a reference could
//! mean, and the blocks under it. An agent reads no page: the same problems
//! reach it as plain text, each with its `help:` line.

use surco::components::{Problem, RunLine, Runs, Said};
use surco::doc::Block;
use yunta_core::diagnostic::{Diagnostic, Report};
use yunta_core::yaml::Location;
use yunta_engine::CheckError;

use super::CliError;
use crate::commands::advice;
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

/// Every problem of a refused document, each quoted from its text, in the
/// order the report has them.
pub(crate) fn document_problems(report: &Report, text: Option<&str>) -> Vec<Problem> {
    hinted(
        report
            .diagnostics
            .iter()
            .map(|diagnostic| found(diagnostic, &report.document.path, text))
            .collect(),
    )
}

/// A problem a page says, and what it names that nothing in a workflow
/// reads, when that is what it is about: a `key` or a `kind`.
pub(crate) struct Pointed {
    problem: Problem,
    unread: Option<&'static str>,
}

impl From<Problem> for Pointed {
    fn from(problem: Problem) -> Self {
        Pointed {
            problem,
            unread: None,
        }
    }
}

impl Pointed {
    /// Where in its file the problem is, as a page orders them: what has
    /// no place comes after everything that has one.
    pub(crate) fn order(&self) -> (usize, usize) {
        self.problem
            .at
            .as_ref()
            .map_or((usize::MAX, 0), |at| (at.line, at.col))
    }
}

/// One problem of a document read from `path`, quoted from `text` at its
/// place when it was read from text, with what helps fix it.
pub(crate) fn found(diagnostic: &Diagnostic, path: &str, text: Option<&str>) -> Pointed {
    use yunta_core::diagnostic::RuleCode;
    let unread = match diagnostic.rule() {
        Some(RuleCode::UnknownKey) => Some("key"),
        Some(RuleCode::UnknownKind) => Some("kind"),
        _ => None,
    };
    Pointed {
        problem: yunta_render::blocks::diagnostic::found(diagnostic, path, text),
        unread,
    }
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

/// `problems` as a page says them, in their order: the command that lists
/// what a workflow reads is said once, under the first that names a key or
/// a kind nothing reads, with what every one of them needs listed.
pub(crate) fn hinted(problems: Vec<Pointed>) -> Vec<Problem> {
    let mut unread: Vec<&str> = Vec::new();
    for what in problems.iter().filter_map(|pointed| pointed.unread) {
        if !unread.contains(&what) {
            unread.push(what);
        }
    }
    let mut hint = (!unread.is_empty()).then(|| {
        format!(
            "`{}` lists every {}",
            advice::schema_workflow(),
            unread.join(" and ")
        )
    });
    problems
        .into_iter()
        .map(|pointed| match pointed.unread {
            Some(_) => pointed.problem.help(hint.take()),
            None => pointed.problem,
        })
        .collect()
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
