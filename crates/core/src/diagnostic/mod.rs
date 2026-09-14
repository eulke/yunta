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

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod artifact;
mod problem;
mod subject;

pub use artifact::{ArtifactCode, ArtifactFailure, FileCode, FileProblem};
pub use problem::{DiagnosticCode, ParseCode, Problem, Rule, RuleCode};
pub use subject::{Named, Subject};

use crate::ArtifactKind;

/// Which document a report is about: the kind that fixes its shape, and
/// where a reader opens it.
///
/// A document always has a kind. An artifact the engine never
/// interprets has no shape to demand and therefore no content to report
/// on; what can go wrong with one is a [`FileProblem`] instead. An
/// artifact another run owes has no document at all, and names none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentRef {
    pub kind: DocumentKind,
    /// As a reader would type it to open the file.
    pub path: String,
}

impl DocumentRef {
    pub fn new(kind: impl Into<DocumentKind>, path: impl Into<String>) -> Self {
        DocumentRef {
            kind: kind.into(),
            path: path.into(),
        }
    }

    /// How the document names itself to a reader.
    pub fn label(&self) -> &'static str {
        self.kind.label()
    }
}

/// What kind of document a report is about.
///
/// Every document this system reads is held to a shape and to rules,
/// and reports what it found the same way — so a workflow that breaks
/// its own rules reaches a reader in the shape a tasks document already
/// does. The two are separate arms because they are read at different
/// moments by different doors: a workflow before any run, an artifact
/// at a node's close.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentKind {
    /// The workflow file a run is created from.
    Workflow,
    /// A document a node produces and the engine reads.
    #[serde(untagged)]
    Artifact(ArtifactKind),
}

impl DocumentKind {
    /// How the kind names itself to a reader.
    pub fn label(self) -> &'static str {
        match self {
            DocumentKind::Workflow => "workflow",
            DocumentKind::Artifact(kind) => kind.label(),
        }
    }

    /// The artifact kind this is, absent for a document that is not an
    /// artifact.
    pub fn artifact(self) -> Option<ArtifactKind> {
        match self {
            DocumentKind::Workflow => None,
            DocumentKind::Artifact(kind) => Some(kind),
        }
    }
}

impl From<ArtifactKind> for DocumentKind {
    fn from(kind: ArtifactKind) -> Self {
        DocumentKind::Artifact(kind)
    }
}

impl fmt::Display for DocumentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocumentKind::Workflow => f.write_str("workflow"),
            DocumentKind::Artifact(kind) => fmt::Display::fmt(kind, f),
        }
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
    pub fn code(&self) -> DiagnosticCode {
        self.problem.code()
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rendered = self.problem.to_string();
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
