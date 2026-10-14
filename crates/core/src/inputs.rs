//! `inputs:` — a workflow's own declared
//! parameters, keyed by name so the schema's own format guarantees
//! uniqueness instead of a validation pass over a `[{name, ...}]` list.
//! One variant per type, `#[serde(tag = "type")]`, so a workflow author
//! only ever sees the fields that apply to the type they picked — the
//! schema itself makes `min`/`max` on a `boolean` input unrepresentable,
//! rather than accepting it and rejecting it later in `check`.

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::diagnostic::Report;
use crate::ids::InputName;
use crate::workflow::ArtifactKind;

/// One entry of `inputs:`. An input is required exactly when it has no
/// `default` — a default is the only way to make one optional. The
/// authored form may also spell that out as `required: true` or
/// `required: false`; a value that contradicts the default is refused
/// where the document is read, and the frozen form carries the default
/// alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    try_from = "AuthoredInputSpec"
)]
pub enum InputSpec {
    String {
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
        default: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// A document of a declared kind, given as a path to it. The `kind`
    /// is what the run reads the file as, so it has no default: a
    /// document whose shape nobody named is a `path`, not a `document`.
    Document {
        kind: crate::workflow::ArtifactKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
}

impl InputSpec {
    /// Whether a run must be given this input: it has no default to
    /// fall back to.
    pub fn is_required(&self) -> bool {
        match self {
            InputSpec::String { default, .. } => default.is_none(),
            InputSpec::Number { default, .. } => default.is_none(),
            InputSpec::Boolean { default, .. } => default.is_none(),
            InputSpec::Enum { default, .. } => default.is_none(),
            InputSpec::Path { default, .. } => default.is_none(),
            InputSpec::Document { default, .. } => default.is_none(),
        }
    }

    pub fn description(&self) -> Option<&str> {
        match self {
            InputSpec::String { description, .. } => description.as_deref(),
            InputSpec::Number { description, .. } => description.as_deref(),
            InputSpec::Boolean { description, .. } => description.as_deref(),
            InputSpec::Enum { description, .. } => description.as_deref(),
            InputSpec::Path { description, .. } => description.as_deref(),
            InputSpec::Document { description, .. } => description.as_deref(),
        }
    }
}

/// `required:` as an author writes it — a statement the `default`
/// already makes, kept only to be checked against it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(from = "bool")]
enum Requiredness {
    #[default]
    Unspoken,
    Required,
    Optional,
}

impl schemars::JsonSchema for Requiredness {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Requiredness".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "boolean" })
    }
}

impl From<bool> for Requiredness {
    fn from(required: bool) -> Self {
        if required {
            Requiredness::Required
        } else {
            Requiredness::Optional
        }
    }
}

impl Requiredness {
    fn check_against(self, has_default: bool) -> Result<(), InputSpecError> {
        match (self, has_default) {
            (Requiredness::Required, true) => Err(InputSpecError::RequiredWithDefault),
            (Requiredness::Optional, false) => Err(InputSpecError::OptionalWithoutDefault),
            _ => Ok(()),
        }
    }
}

/// An input declaration the schema cannot honor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InputSpecError {
    /// A number input's `default`, `min` and `max` are finite: NaN and
    /// the infinities compare with nothing and render as nothing.
    #[error("`{field}` is not a finite number — a number input's default and bounds are finite")]
    NotFinite { field: &'static str },

    /// A `default` is what makes an input optional, so `required: true`
    /// next to one cannot be honored.
    #[error(
        "`required: true` and a `default` contradict each other — the default is what makes \
         an input optional; drop one of them"
    )]
    RequiredWithDefault,
    /// `required: false` with nothing to fall back to would resolve to
    /// no value at all, which no `{{inputs.x}}` render site can
    /// represent.
    #[error(
        "`required: false` with no `default` leaves the input without a value — give it a \
         default, or drop `required: false`"
    )]
    OptionalWithoutDefault,
}

/// The authored form of an input: what the schema accepts, `required:`
/// included, before the contradiction check turns it into an
/// [`InputSpec`].
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum AuthoredInputSpec {
    String {
        #[serde(default)]
        required: Requiredness,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        pattern: Option<String>,
        #[serde(default)]
        min_length: Option<u32>,
    },
    Number {
        #[serde(default)]
        required: Requiredness,
        #[serde(default)]
        default: Option<f64>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
    },
    Boolean {
        #[serde(default)]
        required: Requiredness,
        #[serde(default)]
        default: Option<bool>,
        #[serde(default)]
        description: Option<String>,
    },
    Enum {
        values: Vec<String>,
        #[serde(default)]
        required: Requiredness,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        description: Option<String>,
    },
    Path {
        #[serde(default)]
        required: Requiredness,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        description: Option<String>,
    },
    Document {
        kind: crate::workflow::ArtifactKind,
        #[serde(default)]
        required: Requiredness,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        description: Option<String>,
    },
}

/// The schema is the authored form, `required:` included.
impl schemars::JsonSchema for InputSpec {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "InputSpec".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        AuthoredInputSpec::json_schema(generator)
    }
}

impl TryFrom<AuthoredInputSpec> for InputSpec {
    type Error = InputSpecError;

    fn try_from(authored: AuthoredInputSpec) -> Result<Self, Self::Error> {
        Ok(match authored {
            AuthoredInputSpec::String {
                required,
                default,
                description,
                pattern,
                min_length,
            } => {
                required.check_against(default.is_some())?;
                InputSpec::String {
                    default,
                    description,
                    pattern,
                    min_length,
                }
            }
            AuthoredInputSpec::Number {
                required,
                default,
                description,
                min,
                max,
            } => {
                required.check_against(default.is_some())?;
                for (field, value) in [("default", default), ("min", min), ("max", max)] {
                    if value.is_some_and(|number| !number.is_finite()) {
                        return Err(InputSpecError::NotFinite { field });
                    }
                }
                InputSpec::Number {
                    default,
                    description,
                    min,
                    max,
                }
            }
            AuthoredInputSpec::Boolean {
                required,
                default,
                description,
            } => {
                required.check_against(default.is_some())?;
                InputSpec::Boolean {
                    default,
                    description,
                }
            }
            AuthoredInputSpec::Enum {
                values,
                required,
                default,
                description,
            } => {
                required.check_against(default.is_some())?;
                InputSpec::Enum {
                    values,
                    default,
                    description,
                }
            }
            AuthoredInputSpec::Path {
                required,
                default,
                description,
            } => {
                required.check_against(default.is_some())?;
                InputSpec::Path {
                    default,
                    description,
                }
            }
            AuthoredInputSpec::Document {
                kind,
                required,
                default,
                description,
            } => {
                required.check_against(default.is_some())?;
                InputSpec::Document {
                    kind,
                    default,
                    description,
                }
            }
        })
    }
}

/// Why the inputs a run was handed do not resolve to the inputs its
/// workflow declares. Each variant carries the data that explains it, and
/// the display is a code: a surface writes the words from these fields.
#[derive(Debug, Clone, PartialEq, Error, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputsProblem {
    /// Every input name the workflow does not declare, sorted, so one run
    /// of the command reports them all.
    #[error("unknown")]
    Unknown {
        names: Vec<InputName>,
        declared: Vec<InputName>,
    },
    /// An input with no default that the run was not handed.
    #[error("missing")]
    Missing { name: InputName },
    #[error("invalid_number")]
    InvalidNumber { name: InputName, value: String },
    #[error("invalid_boolean")]
    InvalidBoolean { name: InputName, value: String },
    #[error("below_min")]
    BelowMin {
        name: InputName,
        value: f64,
        min: f64,
    },
    #[error("above_max")]
    AboveMax {
        name: InputName,
        value: f64,
        max: f64,
    },
    #[error("too_short")]
    TooShort {
        name: InputName,
        min_length: u32,
        actual: usize,
    },
    #[error("pattern_mismatch")]
    PatternMismatch {
        name: InputName,
        pattern: String,
        value: String,
    },
    #[error("not_in_enum")]
    NotInEnum {
        name: InputName,
        value: String,
        values: Vec<String>,
    },
    /// An input naming a path that does not exist.
    #[error("path_not_found")]
    PathNotFound { name: InputName, path: String },
    /// A `document` input whose path is there and whose bytes are not: a
    /// directory, or a file the filesystem refuses.
    #[error("document_unreadable")]
    DocumentUnreadable {
        name: InputName,
        path: String,
        /// The label the document's kind names itself with.
        #[serde(deserialize_with = "kind_label")]
        label: String,
        detail: String,
    },
    /// A `document` input whose content is not what its kind declares.
    ///
    /// Carries the report rather than a sentence about it: every problem
    /// the document has reaches the person who has to fix the file, in
    /// the document's own vocabulary, and one correction answers them
    /// all.
    #[error("document_refused")]
    DocumentRefused { name: InputName, report: Report },
    /// Reachable only if a bad `pattern:` slipped past `check` (which
    /// validates every pattern compiles) — resolution reports it as
    /// a typed error rather than panicking on a regex it never saw.
    #[error("invalid_pattern")]
    InvalidPattern {
        name: InputName,
        pattern: String,
        detail: String,
    },
}

/// Reads a document kind's label back, refusing a label no kind names, so a
/// read cannot produce a problem the engine would never have written.
fn kind_label<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let text = String::deserialize(deserializer)?;
    if ArtifactKind::ALL
        .into_iter()
        .any(|kind| kind.label() == text)
    {
        Ok(text)
    } else {
        Err(<D::Error as serde::de::Error>::custom(format!(
            "`{text}` names no document kind"
        )))
    }
}
