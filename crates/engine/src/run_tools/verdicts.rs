//! How an answer to a tool call reads.
//!
//! Every verdict a session gets back is written here, so a refusal about
//! a document, a finding or a file arrives in one shape: what was not
//! accepted, then a numbered list of what to fix, in the words the
//! document's own diagnostics use. An agent that learns to read one
//! refusal can read them all, and a rule reworded in the core is reworded
//! everywhere at once.
//!
//! An acceptance says what the engine *read*, not merely that it parsed:
//! a ledger that comes back as six tasks when the session meant seven is
//! a mistake only the session can still fix.

use yunta_core::diagnostic::Report;
use yunta_core::events::FindingOperation;

/// One document's problems as an instruction to whoever offered it:
/// what was not accepted, and a numbered list of what to fix.
pub(super) fn numbered(heading: String, report: &Report) -> String {
    let mut text = heading;
    for (position, diagnostic) in report.diagnostics.iter().enumerate() {
        text.push_str(&format!("\n\n  {}. {diagnostic}", position + 1));
    }
    text
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

/// `` `a`, `b` `` — how a tool lists the names it takes.
pub(super) fn backticked(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What the engine read out of an artifact, so a session sees its meaning
/// survived the parse and not only its syntax.
pub(super) fn read_as(verified: &crate::artifacts::VerifiedArtifact) -> String {
    use crate::artifacts::ArtifactContent;
    match &verified.content {
        ArtifactContent::Opaque => "Verified by existence and content hash.".to_string(),
        ArtifactContent::TaskLedger(ledger) => format!(
            "{} task(s) registered: {}",
            ledger.tasks.len(),
            names(ledger.tasks.iter().map(|t| t.id.to_string()))
        ),
        ArtifactContent::Findings(findings) => format!(
            "{} finding(s) posted: {}",
            findings.len(),
            names(findings.iter().map(|f| f.id.to_string()))
        ),
        ArtifactContent::Questions(questions) => format!(
            "{} question(s) to answer: {}",
            questions.len(),
            names(questions.iter().map(|q| q.id.to_string()))
        ),
    }
}

fn names(ids: impl Iterator<Item = String>) -> String {
    let ids: Vec<String> = ids.map(|id| format!("`{id}`")).collect();
    if ids.is_empty() {
        "none".to_string()
    } else {
        ids.join(", ")
    }
}
