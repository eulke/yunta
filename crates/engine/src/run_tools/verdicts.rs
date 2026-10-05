//! How an answer to a tool call reads.
//!
//! Every verdict a session gets back is written here, so a refusal about
//! a document, a finding or a file arrives in one shape: what was not
//! accepted, then a numbered list of what to fix, in the words the
//! document's own diagnostics use. An agent that learns to read one
//! refusal can read them all, and a rule reworded in the core is reworded
//! everywhere at once.
//!
//! Every answer, a refusal or not, is a [`Reply`]: the verdict first,
//! then each problem standing in the way, then what to call next — and
//! what a tool answers as data is JSON, written by [`json`].
//!
//! An acceptance says what the engine *read*, not merely that it parsed:
//! a tasks document that comes back as six tasks when the session meant seven is
//! a mistake only the session can still fix.

use serde::Serialize;
use yunta_core::diagnostic::Report;
use yunta_core::events::FindingOperation;

use super::session::RunToolError;

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
        let mut text = self.verdict.clone();
        for (position, problem) in self.problems.iter().enumerate() {
            text.push_str(&format!("\n\n  {}. {problem}", position + 1));
        }
        if let Some(next) = &self.next {
            text.push_str(&format!("\n\nNext: {next}"));
        }
        text
    }
}

/// What a tool answers as data, as the session reads it.
pub(super) fn json(answer: &impl Serialize) -> Result<String, RunToolError> {
    serde_json::to_string_pretty(answer).map_err(|source| RunToolError::Render { source })
}

/// One document's problems as an instruction to whoever offered it:
/// what was not accepted, and a numbered list of what to fix.
pub(super) fn numbered(heading: String, report: &Report) -> String {
    Reply::new(heading)
        .problems(report.diagnostics.iter().map(ToString::to_string))
        .text()
}

/// How a verdict about a file that is there and unreadable opens.
pub(super) fn failure_heading(report: &Report) -> String {
    format!("the {} it holds cannot be read:", report.document.label())
}

pub(super) fn submission_refusal(report: &Report, name: &str) -> String {
    numbered(
        format!(
            "The {} `{name}` was not accepted. Fix these and submit again:",
            report.document.label()
        ),
        report,
    )
}

pub(super) fn refusal(operation: FindingOperation, report: &Report) -> String {
    let heading = match operation {
        FindingOperation::Post => "The finding was not accepted. Fix these and post it again:",
        FindingOperation::Update => {
            "The finding update was not accepted. Fix these and update it again:"
        }
        FindingOperation::Withdraw => {
            "The withdrawal was not accepted. Fix these and withdraw it again:"
        }
    };
    numbered(heading.to_string(), report)
}

/// What the engine read out of an artifact, so a session sees its meaning
/// survived the parse and not only its syntax.
pub(super) fn read_as(verified: &crate::artifacts::VerifiedArtifact) -> String {
    use crate::artifacts::ArtifactContent;
    match &verified.content {
        ArtifactContent::Opaque => "Verified by existence and content hash.".to_string(),
        ArtifactContent::Tasks(tasks) => counted_names(
            tasks.tasks.len(),
            "task",
            "registered",
            tasks.tasks.iter().map(|t| t.id.to_string()),
        ),
        ArtifactContent::Spec(spec) => counted_names(
            spec.specs.len(),
            "task",
            "specified",
            spec.specs.iter().map(|spec| spec.task.to_string()),
        ),
        ArtifactContent::Findings(findings) => counted_names(
            findings.len(),
            "finding",
            "posted",
            findings.iter().map(|f| f.id.to_string()),
        ),
        ArtifactContent::Questions(questions) => counted_names(
            questions.len(),
            "question",
            "to answer",
            questions.iter().map(|q| q.id.to_string()),
        ),
        ArtifactContent::Answers(answers) => counted_names(
            answers.len(),
            "question",
            "answered",
            answers.iter().map(|answer| answer.id.to_string()),
        ),
    }
}

/// `2 questions to answer: `q-a`, `q-b`` — how many of what, what
/// happened to them, and which ones, through the workspace's one
/// counter and its one joiner.
fn counted_names(
    how_many: usize,
    noun: &str,
    happened: &str,
    ids: impl Iterator<Item = String>,
) -> String {
    let ids: Vec<String> = ids.collect();
    let listed = match ids.is_empty() {
        true => "none".to_string(),
        false => yunta_core::text::listed(ids.iter().map(String::as_str)),
    };
    format!(
        "{} {happened}: {listed}",
        yunta_core::text::counted(how_many, noun)
    )
}
