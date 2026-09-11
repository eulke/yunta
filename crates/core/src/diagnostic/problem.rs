//! What is wrong, with everything a correction needs.
//!
//! Each variant carries the facts rather than a sentence, so the words a
//! reader sees are chosen at the edge that knows who is reading.

use serde::{Deserialize, Serialize};

use super::Subject;

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
    /// A second entry already carries this id.
    DuplicateId,
    EmptyTitle,
    EmptyScope,
    NoCriteria,
    /// Every criterion is a guard, so nothing in the task proves work
    /// happened.
    AllCriteriaAreGuards,
    /// `depends_on` names a task nobody declared.
    UnknownDependency,
    DependencyCycle,
    /// Two independent tasks reach for the same files.
    OverlappingScope,
    ManualReviewWithoutJustification,
    EmptyText,
    EmptyLocation,
    EmptyDetail,
    /// `answer_type` is `choice` and `values` is empty.
    MissingValues,
}

impl RuleCode {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleCode::DuplicateId => "duplicate-id",
            RuleCode::EmptyTitle => "empty-title",
            RuleCode::EmptyScope => "empty-scope",
            RuleCode::NoCriteria => "no-criteria",
            RuleCode::AllCriteriaAreGuards => "all-criteria-are-guards",
            RuleCode::UnknownDependency => "unknown-dependency",
            RuleCode::DependencyCycle => "dependency-cycle",
            RuleCode::OverlappingScope => "overlapping-scope",
            RuleCode::ManualReviewWithoutJustification => "manual-review-without-justification",
            RuleCode::EmptyText => "empty-text",
            RuleCode::EmptyLocation => "empty-location",
            RuleCode::EmptyDetail => "empty-detail",
            RuleCode::MissingValues => "missing-values",
        }
    }
}

impl std::fmt::Display for RuleCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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
    /// registration rules and their siblings. `code` is what a receipt
    /// counts; `detail` is the clause a reader acts on.
    Rule {
        code: RuleCode,
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
        matches!(self, Problem::NotYaml { .. } | Problem::Unreadable { .. })
    }

    /// The stable name of this kind of problem: what a receipt counts
    /// and a log is grepped by, unaffected by any rewording.
    pub fn code(&self) -> &'static str {
        match self {
            Problem::NotYaml { .. } => "not-yaml",
            Problem::UnknownKey { .. } => "unknown-key",
            Problem::MissingKey { .. } => "missing-key",
            Problem::WrongShape { .. } => "wrong-shape",
            Problem::InvalidId { .. } => "invalid-id",
            Problem::UnknownValue { .. } => "unknown-value",
            Problem::Rule { code, .. } => code.as_str(),
            Problem::Unreadable { .. } => "unreadable",
        }
    }

    /// The sentence a reader acts on.
    ///
    /// Takes the subject rather than a noun: two of the sentences below
    /// differ for the document itself, and deciding that by comparing a
    /// noun against `"document"` makes rewording the noun silently
    /// switch them off.
    pub(super) fn render(&self, subject: &Subject) -> String {
        let noun = subject.noun();
        match self {
            Problem::NotYaml { looks_like, .. } => match looks_like {
                Some(shape) => format!("is not YAML: {}", shape.advice()),
                None => "is not YAML".to_string(),
            },
            Problem::UnknownKey {
                key,
                valid,
                instead,
            } => unknown_key(subject, key, valid, instead.as_deref()),
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
            Problem::Rule { detail, .. } => detail.clone(),
            Problem::Unreadable { .. } => {
                "could not be read, and the reason could not be narrowed to any entry. \
                 Compare it against the shape above"
                    .to_string()
            }
        }
    }
}

/// An unknown key, with the keys that exist and — when the one written
/// is one a reader plausibly reaches for — what to write instead.
///
/// The document itself is a different sentence from an entry inside it:
/// "a task declares ..." is right for a task and wrong for a file, whose
/// keys are top-level ones.
fn unknown_key(subject: &Subject, key: &str, valid: &[String], instead: Option<&str>) -> String {
    let mut text = format!("unknown key `{key}`");
    match (subject.is_document(), valid.len()) {
        (_, 0) => {}
        (true, 1) => text.push_str(&format!(
            "; the only top-level key is {}",
            backticked(valid)
        )),
        (true, _) => text.push_str(&format!("; the top-level keys are {}", backticked(valid))),
        (false, _) => text.push_str(&format!(
            "; a {} declares {}",
            subject.noun(),
            backticked(valid)
        )),
    }
    if let Some(instead) = instead {
        text.push_str(&format!("; {instead}"));
    }
    text
}

/// `` `a`, `b`, `c` `` — how every diagnostic lists keys.
fn backticked(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("`{item}`"))
        .collect::<Vec<_>>()
        .join(", ")
}
