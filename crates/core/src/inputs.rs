//! `inputs:` — a workflow's own declared
//! parameters, keyed by name so the schema's own format guarantees
//! uniqueness instead of a validation pass over a `[{name, ...}]` list.
//! One variant per type, `#[serde(tag = "type")]`, so a workflow author
//! only ever sees the fields that apply to the type they picked — the
//! schema itself makes `min`/`max` on a `boolean` input unrepresentable,
//! rather than accepting it and rejecting it later in `check`.

use serde::{Deserialize, Serialize};

/// One entry of `inputs:`. `required` and `default` are
/// mutually exclusive by convention, not by the type: `required: true`
/// with no `default` is the ordinary case (and the implicit default when
/// neither field is given — an input the schema is silent about is
/// required, since a `default` is the only way to make one optional).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputSpec {
    String {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        required: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_length: Option<u32>,
    },
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        required: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    Boolean {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        required: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// `values` has no default — an enum with no members can't be parsed
    /// into anything meaningful, so the schema requires at least the
    /// field rather than letting an empty list slip through to `check`.
    Enum {
        values: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        required: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// Always validated for existence at resolution time, no `exists:`
    /// flag and no file/directory distinction — the filesystem
    /// call fails either way, so checking eagerly turns a late, expensive
    /// error (after worktree + baseline + maybe tokens) into an
    /// immediate one. An input naming a path that doesn't exist yet
    /// (an output location) is a `string`, not a `path`.
    Path {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        required: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
}

impl InputSpec {
    /// Whether this input has its own default — the one thing every
    /// variant carries, and the one fact resolution needs before it ever
    /// looks at the type-specific fields.
    pub fn has_default(&self) -> bool {
        match self {
            InputSpec::String { default, .. } => default.is_some(),
            InputSpec::Number { default, .. } => default.is_some(),
            InputSpec::Boolean { default, .. } => default.is_some(),
            InputSpec::Enum { default, .. } => default.is_some(),
            InputSpec::Path { default, .. } => default.is_some(),
        }
    }

    /// The `required:` field as written, independent of whether a
    /// `default` is also present — callers that need to detect the
    /// contradictory `required: true` + `default: ...` combination read
    /// this alongside `has_default()` rather than a single collapsed
    /// bool.
    pub fn required_field(&self) -> Option<bool> {
        match self {
            InputSpec::String { required, .. } => *required,
            InputSpec::Number { required, .. } => *required,
            InputSpec::Boolean { required, .. } => *required,
            InputSpec::Enum { required, .. } => *required,
            InputSpec::Path { required, .. } => *required,
        }
    }

    pub fn description(&self) -> Option<&str> {
        match self {
            InputSpec::String { description, .. } => description.as_deref(),
            InputSpec::Number { description, .. } => description.as_deref(),
            InputSpec::Boolean { description, .. } => description.as_deref(),
            InputSpec::Enum { description, .. } => description.as_deref(),
            InputSpec::Path { description, .. } => description.as_deref(),
        }
    }
}
