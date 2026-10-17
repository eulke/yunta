//! The sentences a submission answers with: what an artifact's verdict reads,
//! what an acceptance tells the session beside what the engine read out of
//! the document, what a refusal lists, and what a session is told about the
//! tests of a spec that fail with no work done.
//!
//! An acceptance says what the engine *read*, not merely that it parsed:
//! a tasks document that comes back as six tasks when the session meant seven is
//! a mistake only the session can fix.

use yunta_core::diagnostic::{ArtifactFailure, Report};
use yunta_core::events::Unanswered;
use yunta_core::{Redactor, TaskId};

use super::verdicts::failure_heading;
use crate::artifacts::{ArtifactContent, VerifiedArtifact};
use crate::run_tools::verdicts::numbered;
use crate::task_cycle::CriterionRun;

/// One artifact's verdict as the session reads it: what the engine
/// understood, or the numbered problems standing in the way.
pub(crate) fn artifact_verdict(
    name: &str,
    verified: Result<VerifiedArtifact, ArtifactFailure>,
) -> String {
    match verified {
        Ok(verified) => format!("{name} — ok. {}", read_as(&verified)),
        Err(failure) => match failure.report() {
            Some(report) => numbered(format!("{name} — {}", failure_heading(report)), report),
            None => format!("{name} — {failure}"),
        },
    }
}

/// An accepted document's reply: what the engine read out of it, beside what
/// the session is told about its tests.
pub(crate) fn accepted(name: &str, verified: &VerifiedArtifact, told: &str) -> String {
    format!("{name} — accepted. {}{told}", read_as(verified))
}

/// A refused document's reply: what was not accepted, and what to fix.
pub(crate) fn submission_refused(report: &Report, name: &str) -> String {
    numbered(
        format!(
            "The {} `{name}` was not accepted. Fix these and submit again:",
            report.document.label()
        ),
        report,
    )
}

/// The next step a session is told when its artifact is one it reports as
/// findings, and a submission of it cannot be read as a document.
pub(crate) fn report_each_finding(post: &str) -> String {
    format!("report each finding with `{post}`")
}

/// What a submission without a document object is told, as the reason it was
/// not read.
pub(crate) fn document_missing() -> &'static str {
    "`document` is missing or is not an object"
}

/// How each test of an accepted spec fails with no work done: its exit and
/// the line it concluded with, redacted the way the log would be. A test
/// that fails for anything but the missing behavior — a typo, a wrong
/// path — fails after the work too, and this is where its writer sees it.
pub(crate) fn failing_now(failing: &[(TaskId, CriterionRun)], redactor: &Redactor) -> String {
    if failing.is_empty() {
        return String::new();
    }
    let mut text = String::from(
        "\n\nHow each test fails now, before any work — one that fails for anything but the \
         missing behavior fails after the work too:",
    );
    for (task, run) in failing {
        let line = match run.conclusion() {
            Some(line) => format!(" — it said `{}`", redactor.text(&line)),
            None => ", printing nothing".to_string(),
        };
        text.push_str(&format!(
            "\n  `{}` (task `{task}`): exit {}{line}",
            run.cmd,
            Unanswered::exit_described(run.exit_code)
        ));
    }
    text
}

/// What the engine read out of an artifact, so a session sees its meaning
/// survived the parse and not only its syntax.
fn read_as(verified: &VerifiedArtifact) -> String {
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
