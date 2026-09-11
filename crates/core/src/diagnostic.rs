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

use crate::ids::TaskId;
use crate::{FindingId, QuestionId};

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

/// Who is at fault, in the vocabulary of the document.
///
/// Every entry carries both its id and its position: the id is how a
/// reader knows which one it is, and the position is the fallback for
/// the case that makes a serde path useless — the id itself is the
/// thing that failed to parse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "of", rename_all = "kebab-case")]
pub enum Subject {
    /// The file as a whole.
    Document,
    Task {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<TaskId>,
        index: usize,
    },
    Criterion {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task: Option<TaskId>,
        index: usize,
    },
    Finding {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<FindingId>,
        index: usize,
    },
    Question {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<QuestionId>,
        index: usize,
    },
}

impl Subject {
    /// The noun an error uses for this kind of entry, so a message can
    /// say "a task declares ..." without the caller knowing which
    /// subject it holds.
    pub fn noun(&self) -> &'static str {
        match self {
            Subject::Document => "document",
            Subject::Task { .. } => "task",
            Subject::Criterion { .. } => "criterion",
            Subject::Finding { .. } => "finding",
            Subject::Question { .. } => "question",
        }
    }
}

/// `the first task`, `the 5th finding` — how an entry is named when its
/// own id could not be read.
fn ordinal(noun: &str, index: usize) -> String {
    let position = index + 1;
    let word = match position {
        1 => "first".to_string(),
        2 => "second".to_string(),
        3 => "third".to_string(),
        n => format!("{n}{}", ordinal_suffix(n)),
    };
    format!("the {word} {noun}")
}

fn ordinal_suffix(n: usize) -> &'static str {
    match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Subject::Document => f.write_str("the document"),
            Subject::Task { id: Some(id), .. } => write!(f, "task `{id}`"),
            Subject::Task { id: None, index } => f.write_str(&ordinal("task", *index)),
            Subject::Criterion {
                task: Some(task),
                index,
            } => write!(f, "task `{task}`, criterion {}", index + 1),
            Subject::Criterion { task: None, index } => {
                write!(f, "criterion {}", index + 1)
            }
            Subject::Finding { id: Some(id), .. } => write!(f, "finding `{id}`"),
            Subject::Finding { id: None, index } => f.write_str(&ordinal("finding", *index)),
            Subject::Question { id: Some(id), .. } => write!(f, "question `{id}`"),
            Subject::Question { id: None, index } => f.write_str(&ordinal("question", *index)),
        }
    }
}

/// What kind of YAML value was found where another was expected. Named
/// as a person writing YAML names them, never as a deserializer names
/// its own types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ValueShape {
    Null,
    Bool,
    Number,
    String,
    Sequence,
    Mapping,
    Tagged,
}

impl ValueShape {
    pub fn label(self) -> &'static str {
        match self {
            ValueShape::Null => "nothing",
            ValueShape::Bool => "a boolean",
            ValueShape::Number => "a number",
            ValueShape::String => "a string",
            ValueShape::Sequence => "a list",
            ValueShape::Mapping => "a mapping",
            ValueShape::Tagged => "a tagged value",
        }
    }
}

/// A shape a reader has met before, recognized so the diagnostic can
/// name the cause instead of the symptom. A stray backtick is a
/// character; a Markdown fence is a mistake with a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Malformation {
    /// The document opens with ```` ``` ````.
    MarkdownFence,
    /// The document is JSON, which YAML mostly accepts but this one did
    /// not.
    JsonDocument,
    /// Prose before the first key — an agent explaining what it wrote.
    LeadingProse,
}

impl Malformation {
    fn advice(self) -> &'static str {
        match self {
            Malformation::MarkdownFence => {
                "it opens with a Markdown code fence. Write the YAML alone, with no fence \
                 around it"
            }
            Malformation::JsonDocument => {
                "it is a JSON document. Write it as YAML: keys followed by `:`, lists as \
                 `-` items"
            }
            Malformation::LeadingProse => {
                "it opens with prose. Write the YAML alone, with no explanation around it"
            }
        }
    }
}

/// What is wrong. Every variant carries what a correction needs, so
/// neither rendering has to guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "problem", rename_all = "kebab-case")]
pub enum Problem {
    /// The bytes are not a YAML document at all.
    NotYaml {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        looks_like: Option<Malformation>,
        /// What the parser said, kept for the log and never rendered to
        /// a reader — a parser's account of its own scanner is not
        /// something anyone can act on.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        detail: String,
    },
    UnknownKey {
        key: String,
        valid: Vec<String>,
        /// What to write instead, when the key is one a reader plausibly
        /// reaches for.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instead: Option<String>,
    },
    MissingKey {
        key: String,
    },
    WrongShape {
        found: ValueShape,
        expected: String,
        /// A line the reader can copy.
        example: String,
    },
    InvalidId {
        value: String,
        rule: String,
    },
    /// A value outside the closed set its key accepts: a severity that
    /// is not on the ladder, an answer type that does not exist.
    UnknownValue {
        value: String,
        valid: Vec<String>,
    },
    /// A rule the document broke once it was readable — the ledger's own
    /// registration rules and their siblings. `code` is stable and
    /// countable; `detail` is the clause a reader acts on.
    Rule {
        code: String,
        detail: String,
    },
    /// Something about the file rather than anything inside it: it was
    /// never written, it is empty, it is past a declared limit. `detail`
    /// completes the sentence "the document ...", because there is no
    /// entry to blame and pointing at one would be an invention.
    File {
        code: String,
        detail: String,
    },
    /// The document was refused and nothing in it could be named as the
    /// cause. That is a gap in the walk, reported as one rather than
    /// swallowed: `detail` carries what the parser said, for the log.
    Unreadable {
        detail: String,
    },
}

impl Problem {
    pub fn not_yaml(looks_like: Option<Malformation>, detail: impl Into<String>) -> Self {
        Problem::NotYaml {
            looks_like,
            detail: detail.into(),
        }
    }

    pub fn unknown_key<I, S>(key: impl Into<String>, valid: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Problem::UnknownKey {
            key: key.into(),
            valid: valid.into_iter().map(Into::into).collect(),
            instead: None,
        }
    }

    /// The same, plus the key that replaced it — the hint table the
    /// workflow parser already keeps for `role:`, available to every
    /// document.
    pub fn unknown_key_instead<I, S>(
        key: impl Into<String>,
        valid: I,
        instead: impl Into<String>,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Problem::UnknownKey {
            key: key.into(),
            valid: valid.into_iter().map(Into::into).collect(),
            instead: Some(instead.into()),
        }
    }

    pub fn missing_key(key: impl Into<String>) -> Self {
        Problem::MissingKey { key: key.into() }
    }

    pub fn wrong_shape(
        found: ValueShape,
        expected: impl Into<String>,
        example: impl Into<String>,
    ) -> Self {
        Problem::WrongShape {
            found,
            expected: expected.into(),
            example: example.into(),
        }
    }

    pub fn invalid_id(value: impl Into<String>, rule: impl Into<String>) -> Self {
        Problem::InvalidId {
            value: value.into(),
            rule: rule.into(),
        }
    }

    pub fn unknown_value<I, S>(value: impl Into<String>, valid: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Problem::UnknownValue {
            value: value.into(),
            valid: valid.into_iter().map(Into::into).collect(),
        }
    }

    pub fn unreadable(detail: impl Into<String>) -> Self {
        Problem::Unreadable {
            detail: detail.into(),
        }
    }

    pub fn rule(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Problem::Rule {
            code: code.into(),
            detail: detail.into(),
        }
    }

    /// `detail` completes the sentence "the document ...".
    pub fn file(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Problem::File {
            code: code.into(),
            detail: detail.into(),
        }
    }

    /// Whether this problem is about the file as a whole, so its
    /// rendering already reads as a sentence about the document and
    /// must not be prefixed with a subject and a colon.
    fn about_document(&self) -> bool {
        matches!(
            self,
            Problem::NotYaml { .. } | Problem::Unreadable { .. } | Problem::File { .. }
        )
    }

    /// The stable name of this kind of problem: what a receipt counts
    /// and a log is grepped by, unaffected by any rewording.
    pub fn code(&self) -> &str {
        match self {
            Problem::NotYaml { .. } => "not-yaml",
            Problem::UnknownKey { .. } => "unknown-key",
            Problem::MissingKey { .. } => "missing-key",
            Problem::WrongShape { .. } => "wrong-shape",
            Problem::InvalidId { .. } => "invalid-id",
            Problem::UnknownValue { .. } => "unknown-value",
            Problem::Rule { code, .. } => code,
            Problem::File { code, .. } => code,
            Problem::Unreadable { .. } => "unreadable",
        }
    }

    /// The sentence a reader acts on. `noun` is the subject's own noun,
    /// so a message can say "a task declares ..." without this type
    /// knowing which subject carries it.
    fn render(&self, noun: &str) -> String {
        match self {
            Problem::NotYaml { looks_like, .. } => match looks_like {
                Some(shape) => format!("is not YAML: {}", shape.advice()),
                None => "is not YAML".to_string(),
            },
            Problem::UnknownKey {
                key,
                valid,
                instead,
            } => {
                let mut text = format!("unknown key `{key}`");
                match (noun, valid.len()) {
                    (_, 0) => {}
                    ("document", 1) => text.push_str(&format!(
                        "; the only top-level key is {}",
                        backticked(valid)
                    )),
                    ("document", _) => {
                        text.push_str(&format!("; the top-level keys are {}", backticked(valid)))
                    }
                    _ => text.push_str(&format!("; a {noun} declares {}", backticked(valid))),
                }
                if let Some(instead) = instead {
                    text.push_str(&format!("; {instead}"));
                }
                text
            }
            Problem::MissingKey { key } => {
                format!("`{key}` is missing; every {noun} declares one")
            }
            Problem::WrongShape {
                found,
                expected,
                example,
            } => format!(
                "expected {expected}, found {}. Write it as `{example}`",
                found.label()
            ),
            Problem::InvalidId { value, rule } => {
                format!("`{value}` is not a valid {noun} id: {rule}")
            }
            Problem::UnknownValue { value, valid } => {
                format!("`{value}` is not one of {}", backticked(valid))
            }
            Problem::Rule { detail, .. } | Problem::File { detail, .. } => detail.clone(),
            Problem::Unreadable { .. } => {
                "could not be read, and the reason could not be narrowed to any entry. \
                 Compare it against the shape above"
                    .to_string()
            }
        }
    }
}

/// `` `a`, `b`, `c` `` — how every diagnostic lists keys.
fn backticked(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("`{item}`"))
        .collect::<Vec<_>>()
        .join(", ")
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
