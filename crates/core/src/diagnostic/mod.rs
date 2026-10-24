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
//! [`surco::text`].

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod artifact;
mod fault;
mod problem;
mod run_fault;
mod subject;

pub use artifact::{ArtifactCode, ArtifactFailure, FileCode, FileProblem};
pub use fault::Fault;
pub use problem::{DiagnosticCode, ParseCode, Problem, Rule, RuleCode, RuleWords};
pub use run_fault::RunFault;
pub use subject::{Named, Subject};

use crate::yaml::{Location, Pointer, SourceMap};
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
    /// Where in the document's text the problem is, when whoever read
    /// the document read it from text: where the parser stopped, or
    /// where the entry the subject names is written. A document handed
    /// over as a structured value has no text, and no place in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Location>,
    /// Where inside the entry the subject names a broken rule is, when
    /// the rule knows the key it is about — what places it in the text.
    /// The subject is what a reader is told; this is never written down.
    #[serde(skip)]
    #[schemars(skip)]
    pub within: Option<Pointer>,
}

impl Diagnostic {
    pub fn new(subject: Subject, problem: Problem) -> Self {
        Diagnostic {
            subject,
            problem,
            at: None,
            within: None,
        }
    }

    /// This diagnostic, about the key `within` of the entry its subject
    /// names.
    pub fn within(mut self, within: Pointer) -> Self {
        self.within = Some(within);
        self
    }

    /// This diagnostic, placed at `at` in its document's text.
    pub fn at(mut self, at: Option<Location>) -> Self {
        self.at = at;
        self
    }

    /// Where in a document of `kind` the problem is, from its root: the
    /// entry the subject names, and inside it the value a parser refused.
    pub fn pointer(&self, kind: DocumentKind) -> Pointer {
        let entry = self.subject.pointer(kind);
        match &self.problem {
            Problem::Parse { path, .. } => entry.join(Pointer::parse_path(path)),
            Problem::Rule { .. } => match &self.within {
                Some(within) => entry.join(within.clone()),
                None => entry,
            },
        }
    }

    /// The stable name of this kind of problem: what a receipt counts
    /// and a log is grepped by, unaffected by any rewording.
    pub fn code(&self) -> DiagnosticCode {
        self.problem.code()
    }

    /// What helps fix the problem, the most direct first: the name a slip
    /// most likely meant, or what took the place of a retired key.
    pub fn help(&self) -> Vec<String> {
        self.problem.help()
    }

    /// The rule the document broke, when the problem is a broken rule.
    pub fn rule(&self) -> Option<RuleCode> {
        match self.code() {
            DiagnosticCode::Rule(code) => Some(code),
            _ => None,
        }
    }
}

impl Diagnostic {
    /// What is wrong, in the document's own words, without where: for a
    /// surface that shows the place itself.
    pub fn said(&self) -> String {
        let rendered = self.problem.to_string();
        // A problem with the file as a whole reads as one sentence
        // ("the document is not YAML: ..."), never as a subject and a
        // restatement of it.
        match &self.subject {
            Subject::Document if self.problem.about_document() => {
                format!("the document {rendered}")
            }
            subject => format!("{subject}: {rendered}"),
        }
    }
}

impl Diagnostic {
    /// What is wrong, and where in the text it is, in words: for a surface
    /// that shows no place.
    pub fn said_unplaced(&self) -> String {
        format!("{}{}", self.said(), self.where_in_words())
    }

    /// Where in its text the problem is, as a line of text says it; nothing
    /// for a problem with no place.
    fn where_in_words(&self) -> String {
        self.at.as_ref().map(Location::in_words).unwrap_or_default()
    }
}

impl From<crate::yaml::YamlError> for Diagnostic {
    /// A refusal of the parser's as the problem the document has: the
    /// value it refused, the name a slip in it most likely meant, and where
    /// the parser stopped.
    fn from(error: crate::yaml::YamlError) -> Self {
        match error {
            crate::yaml::YamlError::Parse(refusal) => {
                let crate::yaml::ParseRefusal {
                    path,
                    message,
                    at,
                    nearest,
                } = *refusal;
                Diagnostic::new(
                    Subject::Document,
                    Problem::parse_nearest(path, message, nearest),
                )
                .at(at)
            }
            other => Diagnostic::new(Subject::Document, Problem::parse("", other.to_string())),
        }
    }
}

impl fmt::Display for Diagnostic {
    /// What is wrong, what helps fix it, and where in the text it is, on
    /// one line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}{}",
            self.said(),
            surco::text::hinted(self.help()),
            self.where_in_words()
        )
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

    /// This report with every problem placed in `text`, the document it
    /// is about, that was not placed already.
    pub fn located(self, text: &str) -> Self {
        self.located_in(|| SourceMap::read(text))
    }

    /// This report with every problem that was not placed already placed
    /// in the map `read` gives, read only when one is not.
    pub fn located_in(mut self, read: impl FnOnce() -> SourceMap) -> Self {
        if self
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.at.is_some())
        {
            return self;
        }
        let map = read();
        let kind = self.document.kind;
        for diagnostic in &mut self.diagnostics {
            if diagnostic.at.is_none() {
                diagnostic.at = map.place(&diagnostic.pointer(kind));
            }
        }
        self
    }
}

impl fmt::Display for Report {
    /// The problems block: the file and the count, then
    /// one violation per line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&surco::text::problems(
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
