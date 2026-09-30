//! What is wrong, with everything a correction needs.
//!
//! Each variant carries the facts rather than a sentence, so the words a
//! reader sees are chosen at the edge that knows who is reading.

use serde::{Deserialize, Serialize};

use super::{ArtifactCode, FileCode};

/// The stable name of a rule that only holds across a whole document.
///
/// Exhaustive, so a rule cannot be minted by typing a new string, and
/// countable, so a receipt reports what a run keeps getting wrong
/// without reading prose. A code says which rule broke and nothing
/// about which document it broke in: `DuplicateId` is one rule asked of
/// tasks documents, findings and questions alike, and what separates the three
/// is the [`ArtifactKind`](crate::ArtifactKind) on the report carrying
/// it. Counting by code alone conflates them; counting by kind and code
/// does not.
/// Declares the rule vocabulary once: the variant, the name it serializes
/// and is grepped by, and the doc a maintainer reads. `ALL` comes from the
/// same list, so a rule cannot be added to the enum without joining the
/// inventory the published contract is built from.
macro_rules! rule_codes {
    ($( $(#[doc = $doc:expr])* $variant:ident => $name:literal ),+ $(,)?) => {
        /// The stable name of a rule that only holds across a whole
        /// document.
        ///
        /// Exhaustive, so a rule cannot be minted by typing a new string,
        /// and countable, so a receipt reports what a run keeps getting
        /// wrong without reading prose. A code says which rule broke and
        /// nothing about which document it broke in: `DuplicateId` is one
        /// rule asked of tasks documents, findings and questions alike, and what
        /// separates the three is the
        /// [`ArtifactKind`](crate::ArtifactKind) on the report carrying
        /// it. Counting by code alone conflates them; counting by kind
        /// and code does not.
        ///
        /// Every variant belongs to some document's
        /// [`RULES`](crate::shape::Document::RULES), so a rule the engine
        /// enforces is a rule the writer was told about before writing.
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            Serialize,
            Deserialize,
            schemars::JsonSchema,
        )]
        #[serde(rename_all = "kebab-case")]
        pub enum RuleCode {
            $( $(#[doc = $doc])* $variant ),+
        }

        impl RuleCode {
            /// Every rule this system can hold a document to.
            pub const ALL: &'static [RuleCode] = &[ $( RuleCode::$variant ),+ ];

            pub fn as_str(self) -> &'static str {
                match self {
                    $( RuleCode::$variant => $name ),+
                }
            }
        }
    };
}

rule_codes! {
    /// A second entry already carries this id.
    DuplicateId => "duplicate-id",
    EmptyTitle => "empty-title",
    EmptyScope => "empty-scope",
    NoCriteria => "no-criteria",
    /// Every criterion is a guard, so nothing in the task proves work
    /// happened.
    AllCriteriaAreGuards => "all-criteria-are-guards",
    /// `depends_on` names a task nobody declared.
    UnknownDependency => "unknown-dependency",
    DependencyCycle => "dependency-cycle",
    /// Two independent tasks reach for the same files.
    OverlappingScope => "overlapping-scope",
    /// A second shape already carries this name.
    DuplicateShape => "duplicate-shape",
    /// A second decision already carries this id.
    DuplicateDecision => "duplicate-decision",
    /// A shape's `owner` names a task nobody declared.
    UnknownShapeOwner => "unknown-shape-owner",
    /// A shape lives in a file its owner's scope does not cover.
    ShapeOutsideOwnerScope => "shape-outside-owner-scope",
    /// A task `uses` a shape nobody declared.
    UnknownShape => "unknown-shape",
    /// A task uses a shape without waiting for the task that builds it.
    ShapeUsedBeforeItsOwner => "shape-used-before-its-owner",
    /// A task says it changes a place its scope does not cover.
    ChangeOutsideScope => "change-outside-scope",
    /// A second spec already names this task.
    DuplicateSpec => "duplicate-spec",
    /// A spec lists no test.
    NoSpecTest => "no-spec-test",
    /// A test does not say what passing it proves.
    UnexplainedTest => "unexplained-test",
    /// A test file's path leaves the repository, or reaches into `.git`.
    TestFileEscapes => "test-file-escapes",
    /// A second file of the document already has this path.
    DuplicateTestFile => "duplicate-test-file",
    /// A spec names a task the run's plan does not declare.
    UnknownSpecTask => "unknown-spec-task",
    /// A criterion's command never answers where the engine runs
    /// criteria: it is not found, not executable, or never returns.
    CriterionCannotRun => "criterion-cannot-run",
    /// A criterion that has to fail before the work already passes.
    CriterionAlreadyPasses => "criterion-already-passes",
    /// A guard that has to pass before the work already fails.
    GuardAlreadyRed => "guard-already-red",
    /// A plan a person reviews says nothing of what it changes in one line.
    NoSummary => "no-summary",
    /// A plan, or one of its tasks, that a person reviews does not say
    /// what it does and why.
    NoDescription => "no-description",
    /// A criterion of a plan a person reviews does not say what it proves.
    UnexplainedCriterion => "unexplained-criterion",
    /// A task of a plan a person reviews does not say what a person sees
    /// once it is done.
    NoOutcome => "no-outcome",
    /// A task of a plan a person reviews does not say what it changes,
    /// where.
    NoChanges => "no-changes",
    /// A decision of a plan a person reviews does not say why.
    UnexplainedDecision => "unexplained-decision",
    EmptyText => "empty-text",
    EmptyDetail => "empty-detail",
    /// An update or a withdrawal names a finding this node never posted.
    UnknownId => "unknown-id",
    /// A withdrawn id is final — it is not posted, updated or withdrawn
    /// again.
    WithdrawnId => "withdrawn-id",
    /// A withdrawal does not say why.
    EmptyReason => "empty-reason",
    /// `answer_type` is `choice` and `values` is empty.
    MissingValues => "missing-values",
    /// A `required` question has no answer.
    MissingAnswer => "missing-answer",
    /// An answer's value is not what its question's `answer_type` allows.
    MismatchedAnswer => "mismatched-answer",
    /// A `mode` leaves out a node the workflow cannot run without, or a
    /// node a node it includes reroutes to.
    IncoherentMode => "incoherent-mode",
    /// `invariant: true` on a child of a `parallel` group, where nothing
    /// honors it: modes and re-verification read the top level.
    InvariantInParallel => "invariant-in-parallel",
    /// `optional: true` where leaving the node out would leave the run
    /// without something it cannot do without: on a child of a
    /// `parallel` group, or on a node a required one re-routes to or
    /// reads from.
    IncoherentOptional => "incoherent-optional",
}

impl std::fmt::Display for RuleCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One rule a document must satisfy, stated where it is enforced.
///
/// A rule has two readings and one statement. Before anything is written it
/// is what the document must satisfy, published with the shape; after, it is
/// the diagnostic naming what was broken. Keeping the statement next to the
/// code that enforces it is what stops the two readings drifting — a rule a
/// writer never heard of is a whole attempt spent on something the
/// system already knew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub code: RuleCode,
    /// What it demands, in the vocabulary of whoever writes the document.
    /// One clause, no leading dash, no trailing period.
    pub demand: &'static str,
}

/// What is wrong. Every variant carries what a correction needs, so
/// neither rendering has to guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "problem", rename_all = "kebab-case")]
pub enum Problem {
    /// The document does not parse into its kind: an unknown key, a
    /// value of the wrong type, an id that is not one. `path` locates
    /// the offending value from the document's root (`tasks[1].scope`),
    /// and is empty when the root itself is the problem; `message` is
    /// what the deserializer said about that value, which names the key
    /// and what it expected.
    Parse {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        path: String,
        message: String,
    },
    /// A rule the document broke once it was readable — the tasks document's own
    /// registration rules and their siblings. `code` is what a receipt
    /// counts; `detail` is the clause a reader acts on.
    Rule { code: RuleCode, detail: String },
}

impl Problem {
    /// A value the deserializer refused, at the path it refused it.
    pub fn parse(path: impl Into<String>, message: impl Into<String>) -> Self {
        Problem::Parse {
            path: path.into(),
            message: message.into(),
        }
    }

    pub fn rule(code: RuleCode, detail: impl Into<String>) -> Self {
        Problem::Rule {
            code,
            detail: detail.into(),
        }
    }

    /// Whether this problem is about the file as a whole, so its
    /// rendering already reads as a sentence about the document and
    /// must not be prefixed with a subject and a colon.
    pub(super) fn about_document(&self) -> bool {
        matches!(self, Problem::Parse { .. })
    }

    /// The stable name of this kind of problem: what a receipt counts
    /// and a log is grepped by, unaffected by any rewording.
    pub fn code(&self) -> DiagnosticCode {
        match self {
            Problem::Parse { .. } => DiagnosticCode::Parse(ParseCode::Parse),
            Problem::Rule { code, .. } => DiagnosticCode::Rule(*code),
        }
    }
}

/// The stable name of anything this system reports as wrong: what a
/// receipt counts, what `status --json` publishes, and what a log is
/// grepped by, unaffected by any rewording.
///
/// Closed, so a code cannot be minted by typing a string. The four arms
/// are the four ways something can be wrong: a rule a readable document
/// broke, a document that did not read at all, a file the close could
/// not take, and an artifact nobody handed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiagnosticCode {
    Rule(RuleCode),
    Parse(ParseCode),
    File(FileCode),
    Artifact(ArtifactCode),
}

impl DiagnosticCode {
    /// Every code this system can report, which is what the published
    /// vocabulary is checked against.
    pub fn all() -> Vec<DiagnosticCode> {
        RuleCode::ALL
            .iter()
            .map(|code| DiagnosticCode::Rule(*code))
            .chain([DiagnosticCode::Parse(ParseCode::Parse)])
            .chain(FileCode::ALL.map(DiagnosticCode::File))
            .chain(ArtifactCode::ALL.map(DiagnosticCode::Artifact))
            .collect()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticCode::Rule(code) => code.as_str(),
            DiagnosticCode::Parse(code) => code.as_str(),
            DiagnosticCode::File(code) => code.as_str(),
            DiagnosticCode::Artifact(code) => code.as_str(),
        }
    }
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One published name, one wire form: every surface carries the flat
/// string, never the shape the union has in Rust.
impl Serialize for DiagnosticCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The one way a document fails before any rule can be asked of it: its
/// bytes are not the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParseCode {
    Parse,
}

impl ParseCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ParseCode::Parse => "parse",
        }
    }
}

/// What is wrong, as one clause. A reader that has no document to name
/// — a rule about a value the engine holds rather than about a file it
/// read — takes the clause on its own, and takes it from here rather
/// than composing its own from the variants.
impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::Parse { path, message } => match path.as_str() {
                "" | "." => write!(f, "does not parse: {message}"),
                path => write!(f, "does not parse at `{path}`: {message}"),
            },
            Problem::Rule { detail, .. } => f.write_str(detail),
        }
    }
}
