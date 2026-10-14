//! How an answer to a tool call is shaped.
//!
//! Every verdict a session gets back has one shape, so a refusal about a
//! document, a finding or a file arrives as what was not accepted, then a
//! numbered list of what to fix. An agent that learns to read one refusal
//! can read them all, and a rule reworded in the core is reworded everywhere
//! at once.
//!
//! Every answer, a refusal or not, is a [`Reply`]: the verdict first,
//! then each problem standing in the way, then what to call next — and
//! what a tool answers as data is JSON, written by [`json`]. The sentences
//! of a reply are written in `words`, the one module an agent's text comes
//! from.

use serde::Serialize;
use yunta_core::diagnostic::Report;
use yunta_core::events::FindingOperation;

use super::session::RunToolError;
use super::words;

/// What a session is told back, in the one shape every tool answers in:
/// the verdict first, then each problem standing in its way, numbered,
/// then what to call next. An agent that has read one answer can read
/// them all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Reply {
    verdict: String,
    problems: Vec<String>,
    next: Option<String>,
}

impl Reply {
    /// An answer that says `verdict`.
    pub(super) fn new(verdict: impl Into<String>) -> Self {
        Reply {
            verdict: verdict.into(),
            problems: Vec::new(),
            next: None,
        }
    }

    /// The same answer, with what stands in the way, in order.
    pub(super) fn problems(mut self, problems: impl IntoIterator<Item = String>) -> Self {
        self.problems.extend(problems);
        self
    }

    /// The same answer, saying what to do next.
    pub(super) fn next(mut self, next: impl Into<String>) -> Self {
        self.next = Some(next.into());
        self
    }

    /// The text the session reads.
    pub(super) fn text(&self) -> String {
        words::reply_text(&self.verdict, &self.problems, self.next.as_deref())
    }
}

/// What a tool answers as data, as the session reads it.
pub(super) fn json(answer: &impl Serialize) -> Result<String, RunToolError> {
    serde_json::to_string_pretty(answer).map_err(|source| RunToolError::Render { source })
}

/// One document's problems as an instruction to whoever offered it: what
/// was not accepted, and a numbered list of what to fix.
pub(super) fn numbered(heading: String, report: &Report) -> String {
    Reply::new(heading)
        .problems(report.diagnostics.iter().map(ToString::to_string))
        .text()
}

/// What a finding's refused operation reads back: the heading that says what
/// was not accepted, then the numbered problems to fix.
pub(super) fn refusal(operation: FindingOperation, report: &Report) -> String {
    numbered(
        words::finding_refusal_heading(operation).to_string(),
        report,
    )
}
