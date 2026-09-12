//! What is wrong with a document an agent wrote, as data.
//!
//! A diagnostic is a value, not a sentence. It names the entry at fault
//! in the vocabulary of the document — `task `t1`, criterion 1`, never
//! the path a deserializer walked — and carries what a correction needs
//! rather than the words for it. That is what lets one problem reach a
//! person as a line in a block, an agent as an instruction to rewrite,
//! and a receipt as a number, without any of the three restating the
//! others.
//!
//! The module is split the way the question splits: [`Subject`] is who
//! is at fault, [`Problem`] is what is wrong, [`ArtifactFailure`] is why
//! one declared artifact did not close. Layout — how a surface wraps,
//! indents or counts what it is told — is not here; it is
//! [`crate::text`].

use std::fmt;

use serde::{Deserialize, Serialize};

mod artifact;
mod problem;
mod subject;

pub use artifact::{ArtifactFailure, FileProblem};
pub use problem::{Problem, Rule, RuleCode};
pub use subject::{Named, Subject};

use crate::ArtifactKind;

/// Which document a report is about: the kind that fixes its shape, and
/// where a reader opens it.
///
/// A document always has a kind. An artifact the engine never
/// interprets has no shape to demand and therefore no content to report
/// on; what can go wrong with one is a [`FileProblem`] instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentRef {
    pub kind: ArtifactKind,
    /// As a reader would type it to open the file.
    pub path: String,
}

impl DocumentRef {
    pub fn new(kind: ArtifactKind, path: impl Into<String>) -> Self {
        DocumentRef {
            kind,
            path: path.into(),
        }
    }

    /// How the document names itself to a reader.
    pub fn label(&self) -> &'static str {
        self.kind.label()
    }
}

/// One thing that is wrong with a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Diagnostic {
    #[serde(flatten)]
    pub subject: Subject,
    #[serde(flatten)]
    pub problem: Problem,
}

impl Diagnostic {
    pub fn new(subject: Subject, problem: Problem) -> Self {
        Diagnostic { subject, problem }
    }

    /// The stable name of this kind of problem: what a receipt counts
    /// and a log is grepped by, unaffected by any rewording.
    pub fn code(&self) -> &'static str {
        self.problem.code()
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rendered = self.problem.render(&self.subject);
        // A problem with the file as a whole reads as one sentence
        // ("the document is not YAML: ..."), never as a subject and a
        // restatement of it.
        match &self.subject {
            Subject::Document if self.problem.about_document() => {
                write!(f, "the document {rendered}")
            }
            subject => write!(f, "{subject}: {rendered}"),
        }
    }
}

/// Every problem one document has, reported together.
///
/// Collecting them is not a convenience: a reader who corrects one
/// problem per round pays a round per problem, and a writer who hears
/// one rule at a time rewrites the document once per rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Report {
    pub document: DocumentRef,
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub fn new(document: DocumentRef, diagnostics: Vec<Diagnostic>) -> Self {
        Report {
            document,
            diagnostics,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

impl fmt::Display for Report {
    /// The block `spec-ledger.md` §4 fixes: the file and the count, then
    /// one violation per line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&crate::text::problems(
            &self.document.path,
            &self.diagnostics,
        ))
    }
}

/// A report is what reading an interpreted document fails with, so it
/// is an error in the language's own terms: a caller can `?` it, and
/// anything that wraps it keeps it as a source rather than flattening
/// it to a string.
impl std::error::Error for Report {}
