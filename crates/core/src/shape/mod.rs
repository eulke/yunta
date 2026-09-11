//! Reading a document an agent wrote, and publishing the shape it
//! should have written.
//!
//! [`read`] parses with serde on the happy path, so the schema is
//! declared exactly once — in the types — and never restated here. Only
//! when serde refuses does the document get walked a second time, by
//! [`Shaped::diagnose`], whose single job is to name every problem at
//! once in the vocabulary of the document. A reader that corrects one
//! problem per round pays a round per problem; a repair cycle that
//! works that way exhausts its budget before the file is readable.
//!
//! [`Shaped::EXAMPLE`] is the same shape as prose a writer can copy. It
//! is what every door publishes: the block a node's session receives,
//! the `document_shape` tool, `yunta schema`, and the shape carried
//! inside a failed read's own report. One text with four consumers,
//! rather than four texts that drift apart at the first schema change.

use serde::de::DeserializeOwned;

use crate::diagnostic::{
    Diagnostic, DocumentKind, DocumentRef, Malformation, Problem, Report, Subject,
};
use crate::yaml::Value;
use crate::{FindingsFile, Ledger, QuestionsFile};

// The published shapes are documents, not code: they live as the YAML
// files they are, where an editor reads them as YAML and a person
// reviewing a schema change sees the diff in the format the change is
// about. `include_str!` binds them at compile time, so they are still
// constants and the tests that read them back through `read` still catch
// any drift from the parser.
const LEDGER_EXAMPLE: &str = include_str!("shapes/task-ledger.yaml");
const FINDINGS_EXAMPLE: &str = include_str!("shapes/findings.yaml");
const QUESTIONS_EXAMPLE: &str = include_str!("shapes/questions.yaml");

mod documents;
mod walk;

/// A document whose shape the system publishes and whose failures it
/// explains.
pub trait Shaped: DeserializeOwned {
    const KIND: DocumentKind;

    /// A complete, valid document of this kind, annotated field by
    /// field. Every door hands this to whoever has to write one; the
    /// test that reads it back through [`read`] is what keeps it true.
    const EXAMPLE: &'static str;

    /// Every problem this document has, in document order. Called only
    /// when serde has already refused the document, so it never has to
    /// build a value — only to explain one.
    fn diagnose(value: &Value, into: &mut Vec<Diagnostic>);
}

/// The shape published for a kind: what every door hands to whoever has
/// to write one. The single place a kind maps to its text, so no door
/// can render a different one.
pub fn published(kind: DocumentKind) -> &'static str {
    match kind {
        DocumentKind::TaskLedger => Ledger::EXAMPLE,
        DocumentKind::Findings => FindingsFile::EXAMPLE,
        DocumentKind::Questions => QuestionsFile::EXAMPLE,
    }
}

/// Every kind a door can be asked about.
pub const KINDS: [DocumentKind; 3] = [
    DocumentKind::TaskLedger,
    DocumentKind::Findings,
    DocumentKind::Questions,
];

/// Reads `bytes` into `T`, or reports every problem the document has.
pub fn read<T: Shaped>(bytes: &[u8], document: DocumentRef) -> Result<T, Report> {
    let one = |problem: Problem| {
        Report::new(
            document.clone(),
            vec![Diagnostic::new(Subject::Document, problem)],
        )
    };

    let Ok(text) = std::str::from_utf8(bytes) else {
        return Err(one(Problem::not_yaml(None, "the bytes are not UTF-8")));
    };

    let refusal = match crate::yaml::parse::<T>(text) {
        Ok(value) => return Ok(value),
        Err(error) => error.to_string(),
    };

    // Whether the bytes are YAML at all decides which explanation is
    // honest: a document that never parsed has no entries to blame.
    let Ok(value) = crate::yaml::parse::<Value>(text) else {
        return Err(one(Problem::not_yaml(looks_like(text), refusal)));
    };

    let mut diagnostics = Vec::new();
    T::diagnose(&value, &mut diagnostics);
    if diagnostics.is_empty() {
        // The walk found nothing serde objected to. That is a gap in
        // this module, and it is reported as one rather than swallowed:
        // a read that fails always names at least one problem.
        diagnostics.push(Diagnostic::new(
            Subject::Document,
            Problem::unreadable(refusal),
        ));
    }
    Err(Report::new(document, diagnostics))
}

/// A malformation a writer recognizes, so the diagnostic can name the
/// cause instead of the character the scanner tripped on.
fn looks_like(text: &str) -> Option<Malformation> {
    let trimmed = text.trim_start();
    if trimmed.starts_with("```") {
        return Some(Malformation::MarkdownFence);
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Some(Malformation::JsonDocument);
    }
    // A first line with neither a key nor a list item is prose: an
    // agent explaining the file it is about to write.
    let first = trimmed.lines().find(|line| !line.trim().is_empty())?;
    let looks_structural =
        first.contains(':') || first.trim_start().starts_with('-') || first.starts_with("---");
    (!looks_structural).then_some(Malformation::LeadingProse)
}
