//! What is wrong, with everything a correction needs.
//!
//! Each variant carries the facts rather than a sentence, so the words a
//! reader sees are chosen at the edge that knows who is reading.

use serde::{Deserialize, Serialize};

use super::Subject;

/// The stable name of a rule that only holds across a whole document.
///
/// Exhaustive, so a rule cannot be minted by typing a new string, and
/// countable, so a receipt reports what a run keeps getting wrong
/// without reading prose. A code says which rule broke and nothing
/// about which document it broke in: `DuplicateId` is one rule asked of
/// ledgers, findings and questions alike, and what separates the three
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
        /// rule asked of ledgers, findings and questions alike, and what
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
    ManualReviewWithoutJustification => "manual-review-without-justification",
    EmptyText => "empty-text",
    EmptyLocation => "empty-location",
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
    /// the offending value from the document's root
    /// (`tasks[1].manual_review`), and is empty when the root itself is
    /// the problem; `message` is what the deserializer said about that
    /// value, which names the key and what it expected.
    Parse {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        path: String,
        message: String,
    },
    /// A rule the document broke once it was readable — the ledger's own
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
    pub fn code(&self) -> &'static str {
        match self {
            Problem::Parse { .. } => "parse",
            Problem::Rule { code, .. } => code.as_str(),
        }
    }

    /// The sentence a reader acts on.
    ///
    /// Takes the subject rather than a noun: two of the sentences below
    /// differ for the document itself, and deciding that by comparing a
    /// noun against `"document"` makes rewording the noun silently
    /// switch them off.
    pub(super) fn render(&self, _subject: &Subject) -> String {
        match self {
            Problem::Parse { path, message } => match path.as_str() {
                "" | "." => format!("does not parse: {message}"),
                path => format!("does not parse at `{path}`: {message}"),
            },
            Problem::Rule { detail, .. } => detail.clone(),
        }
    }
}
