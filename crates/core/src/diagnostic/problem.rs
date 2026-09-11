//! What is wrong, with everything a correction needs.
//!
//! Each variant carries the facts rather than a sentence, so the words a
//! reader sees are chosen at the edge that knows who is reading.

use serde::{Deserialize, Serialize};

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
    pub(super) fn about_document(&self) -> bool {
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
    pub(super) fn render(&self, noun: &str) -> String {
        match self {
            Problem::NotYaml { looks_like, .. } => match looks_like {
                Some(shape) => format!("is not YAML: {}", shape.advice()),
                None => "is not YAML".to_string(),
            },
            Problem::UnknownKey {
                key,
                valid,
                instead,
            } => unknown_key(noun, key, valid, instead.as_deref()),
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

/// An unknown key, with the keys that exist and — when the one written
/// is one a reader plausibly reaches for — what to write instead.
///
/// The document itself is a different sentence from an entry inside it:
/// "a task declares ..." is right for a task and wrong for a file, whose
/// keys are top-level ones.
fn unknown_key(noun: &str, key: &str, valid: &[String], instead: Option<&str>) -> String {
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

/// `` `a`, `b`, `c` `` — how every diagnostic lists keys.
fn backticked(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("`{item}`"))
        .collect::<Vec<_>>()
        .join(", ")
}
