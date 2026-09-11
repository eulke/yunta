//! What is wrong with a document, as a value.
//!
//! Every frontier that reads an authored document reports its failures
//! as [`Diagnostic`]s collected into a [`Report`]. A diagnostic keeps
//! the three things a correction needs apart — who is at fault, where,
//! and what is wrong — so the text a reader sees is produced at the
//! edge that knows who is reading, and never at the point of failure.
//!
//! Two readings exist, and they are the whole reason this is a value
//! and not a sentence: [`Report::for_person`] renders the block
//! `spec-ledger.md` §4 fixes, and [`Report::for_agent`] renders the
//! same facts as an instruction to rewrite the file, with the shape the
//! document should have had.
//!
//! A subject is named the way the document names it — `task \`t1\``,
//! `task \`t1\`, criterion 1` — never by the path a deserializer walked
//! to reach it. A reader who writes YAML has no way to act on
//! `tasks[0].criteria[0]`.

use std::fmt;

use serde::{Deserialize, Serialize};

mod problem;
mod subject;

pub use problem::{Malformation, Problem, ValueShape};
pub use subject::Subject;

/// A document Yunta reads and validates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentKind {
    TaskLedger,
    Findings,
    Questions,
}

impl DocumentKind {
    /// How the kind names itself to a reader.
    pub fn label(self) -> &'static str {
        match self {
            DocumentKind::TaskLedger => "task ledger",
            DocumentKind::Findings => "findings artifact",
            DocumentKind::Questions => "questions artifact",
        }
    }

    /// The value `kind:` carries in a workflow, and the argument
    /// `yunta schema` takes.
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentKind::TaskLedger => "task-ledger",
            DocumentKind::Findings => "findings",
            DocumentKind::Questions => "questions",
        }
    }
}

impl fmt::Display for DocumentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<crate::ArtifactKind> for DocumentKind {
    fn from(kind: crate::ArtifactKind) -> Self {
        match kind {
            crate::ArtifactKind::TaskLedger => DocumentKind::TaskLedger,
            crate::ArtifactKind::Findings => DocumentKind::Findings,
            crate::ArtifactKind::Questions => DocumentKind::Questions,
        }
    }
}

/// Which document a report is about: its kind, and where a reader finds
/// it.
///
/// `kind` is absent for an artifact the engine never interprets. That is
/// not a gap: an opaque artifact has no shape to demand, so a problem
/// with one can only ever be about the file itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<DocumentKind>,
    /// As a reader would type it to open the file.
    pub path: String,
}

impl DocumentRef {
    pub fn new(kind: impl Into<Option<DocumentKind>>, path: impl Into<String>) -> Self {
        DocumentRef {
            kind: kind.into(),
            path: path.into(),
        }
    }

    /// How the document names itself to a reader: its kind when it has
    /// one, otherwise just an artifact.
    pub fn label(&self) -> &'static str {
        match self.kind {
            Some(kind) => kind.label(),
            None => "artifact",
        }
    }
}

/// Where in the document the problem is, when the document is a file
/// the reader can open. One-based, as an editor counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

/// One thing that is wrong with a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Diagnostic {
    pub subject: Subject,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Span>,
    #[serde(flatten)]
    pub problem: Problem,
}

impl Diagnostic {
    pub fn new(subject: Subject, problem: Problem) -> Self {
        Diagnostic {
            subject,
            at: None,
            problem,
        }
    }

    /// The same diagnostic, located at a line and column of the file.
    pub fn at(mut self, line: usize, column: usize) -> Self {
        self.at = Some(Span { line, column });
        self
    }

    /// The stable name of this kind of problem.
    pub fn code(&self) -> &str {
        self.problem.code()
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rendered = self.problem.render(self.subject.noun());
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

/// A failure on one line, for a place that has room for exactly one: a
/// graph label, a row in a listing, a summary.
///
/// A failure that names several problems carries newlines, so every such
/// place needs this collapse. It lives here, once, rather than in each
/// of them — a renderer that forgets it does not produce a worse line,
/// it produces a broken table or an unparseable graph.
pub fn single_line(outcome: &str) -> String {
    outcome.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A failure as a block under the line that introduces it, so every
/// problem keeps the line it was written on.
///
/// The first line follows the introduction; the rest are indented to sit
/// under it. `indent` is what the surface uses for nesting.
pub fn block(outcome: &str, indent: &str) -> String {
    let mut lines = outcome.lines();
    let Some(first) = lines.next() else {
        return String::new();
    };
    let mut text = first.to_string();
    for line in lines {
        text.push('\n');
        text.push_str(indent);
        text.push_str(line);
    }
    text
}

/// Every problem one document has, reported together.
///
/// Collecting them is not a convenience: a reader who corrects one
/// problem per round pays a round per problem, and a repair cycle that
/// works that way exhausts its budget before the file is readable.
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

    /// Whether writing the document again could fix every problem in it.
    ///
    /// False when any problem is about the file rather than its content:
    /// an artifact never produced, an empty one, one past a declared
    /// limit. There is nothing in those to correct, and a limit is a
    /// guard against accidents rather than something to negotiate with.
    pub fn is_repairable(&self) -> bool {
        !self.diagnostics.is_empty()
            && !self
                .diagnostics
                .iter()
                .any(|diagnostic| matches!(diagnostic.problem, Problem::File { .. }))
    }

    /// The block a person reads, in the shape `spec-ledger.md` §4 fixes:
    /// the file and the count, then one violation per line.
    pub fn for_person(&self) -> String {
        let mut text = format!(
            "{}: {} {}",
            self.document.path,
            self.diagnostics.len(),
            if self.diagnostics.len() == 1 {
                "error"
            } else {
                "errors"
            }
        );
        for diagnostic in &self.diagnostics {
            text.push_str(&format!("\n  {diagnostic}"));
        }
        text
    }

    /// The same facts as an instruction to whoever wrote the file, with
    /// the shape it should have had — so a writer that never saw the
    /// shape still converges on the next attempt. `shape` is absent for
    /// a document with no declared shape, and then the instruction
    /// stands on the problems alone.
    pub fn for_agent(&self, shape: Option<&str>) -> String {
        let mut text = format!(
            "The {} you wrote at {} could not be read. Fix these and write the file again:\n",
            self.document.label(),
            self.document.path
        );
        for (position, diagnostic) in self.diagnostics.iter().enumerate() {
            text.push_str(&format!("\n  {}. {diagnostic}", position + 1));
        }
        if let Some(shape) = shape {
            text.push_str("\n\nThe shape it must have:\n\n");
            text.push_str(shape);
        }
        text
    }
}
