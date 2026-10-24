//! The one place YAML enters and leaves the system.
//!
//! Every file a person or an agent writes — workflows, config layers,
//! pack manifests, test cases, mock fixtures, tasks documents, findings,
//! questions — parses through [`parse`]; what the engine persists and
//! reads back (manifests, locks) parses through the same function with
//! types that tolerate what they do not know. A parse error names the
//! path of the value that failed (`nodes[2].artifacts.produces[0]`), so
//! a caller only ever adds the file.

use serde::de::DeserializeOwned;
use serde::Serialize;

mod source;
mod value;

pub use source::{Location, Pointer, ReadAs, SourceMap, Step};
pub use value::{Mapping, Number, Value};

/// A YAML document that could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum YamlError {
    /// The document does not parse into the expected type: what the
    /// parser refused, in a box so that every error carrying one stays
    /// small.
    #[error("{0}")]
    Parse(Box<ParseRefusal>),
    #[error("not valid UTF-8: {source}")]
    Utf8 {
        #[source]
        source: std::str::Utf8Error,
    },
    #[error("cannot serialize: {message}")]
    Serialize { message: String },
}

/// What the parser refused, and where. `path` locates the offending value
/// from the document's root; it is empty when the root itself is the
/// problem. `at` is where in the text the parser stopped, when it was
/// reading text.
#[derive(Debug)]
pub struct ParseRefusal {
    pub path: String,
    pub message: String,
    pub at: Option<Location>,
    /// The name the refused one is most likely a slip from.
    pub nearest: Option<String>,
}

impl From<ParseRefusal> for YamlError {
    fn from(refusal: ParseRefusal) -> Self {
        YamlError::Parse(Box::new(refusal))
    }
}

impl std::fmt::Display for ParseRefusal {
    /// Where in the document, what was refused, the name a slip most
    /// likely meant, and where in the text the parser stopped, on one line.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let said = format!(
            "{}{}{}",
            self.message,
            surco::text::near_miss(self.nearest.as_deref()),
            self.at.as_ref().map(Location::in_words).unwrap_or_default()
        );
        match self.path.as_str() {
            "" | "." => f.write_str(&said),
            path => write!(f, "`{path}`: {said}"),
        }
    }
}

/// Parses `text` into `T`. An error locates the failing value by its
/// path from the document's root, and keeps the parser's own line and
/// column.
pub fn parse<T: DeserializeOwned>(text: &str) -> Result<T, YamlError> {
    let mut path = String::new();
    serde_saphyr::with_deserializer_from_str_with_options(text, reading(), |deserializer| {
        serde_path_to_error::deserialize(deserializer).map_err(|error| {
            path = error.path().to_string();
            error.into_inner()
        })
    })
    .map_err(|error| {
        YamlError::from(ParseRefusal {
            path,
            message: said(&error),
            at: error.location().map(|at| Location {
                line: at.line() as usize,
                col: at.column() as usize,
                len: at.span().len() as usize,
            }),
            nearest: slip(&error),
        })
    })
}

/// The name a refused field or variant is most likely a slip from, among
/// the ones serde expected there.
fn slip(error: &serde_saphyr::Error) -> Option<String> {
    use serde_saphyr::Error;
    match error {
        Error::SerdeUnknownField {
            field, expected, ..
        } => near(field, expected),
        Error::SerdeUnknownVariant {
            variant, expected, ..
        } => near(variant, expected),
        _ => None,
    }
}

/// What a reader is told when a document does not read: the refusal in
/// serde's own words. Where in the text it is travels beside it.
fn said(error: &serde_saphyr::Error) -> String {
    use serde_saphyr::MessageFormatter;
    Said.format_message(error)
        .chars()
        .flat_map(|c| match c.is_control() {
            true => c.escape_debug().collect::<Vec<_>>(),
            false => vec![c],
        })
        .collect()
}

/// The refusals serde words for every format, in those words — the names
/// a document may use in backticks, the way every message here quotes
/// one — and the rest as the parser says them to a person. Never the
/// parser's advice about its own options, which a reader of a workflow
/// has no say over.
struct Said;

impl serde_saphyr::MessageFormatter for Said {
    fn format_message<'a>(&self, error: &'a serde_saphyr::Error) -> std::borrow::Cow<'a, str> {
        use serde_saphyr::Error;
        match error {
            Error::SerdeUnknownField {
                field, expected, ..
            } => format!("unknown field `{field}`, {}", one_of(expected)).into(),
            Error::SerdeUnknownVariant {
                variant, expected, ..
            } => format!("unknown variant `{variant}`, {}", one_of(expected)).into(),
            Error::DuplicateMappingKey { key: Some(key), .. } => {
                format!("`{key}` is written twice in one mapping").into()
            }
            Error::DuplicateMappingKey { key: None, .. } => {
                "a key is written twice in one mapping".into()
            }
            other => serde_saphyr::UserMessageFormatter.format_message(other),
        }
    }
}

/// The name `typed` most likely misspells, when there is a choice to
/// make: with one name expected, the refusal already names it.
fn near(typed: &str, expected: &[&str]) -> Option<String> {
    match expected {
        [_, _, ..] => surco::text::nearest(typed, expected.iter().copied()).map(str::to_string),
        _ => None,
    }
}

/// The names a refused value could have been, as serde says them.
fn one_of(expected: &[&str]) -> String {
    match expected {
        [] => "there are none".to_string(),
        [only] => format!("expected `{only}`"),
        [first, second] => format!("expected `{first}` or `{second}`"),
        several => format!(
            "expected one of {}",
            several
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// How every document is read: YAML 1.2, where only `true` and `false`
/// are booleans — so a node named `n` and a gate option `on` are the
/// words they look like — and a key written twice is refused rather
/// than one of its values dropped. A failure is a sentence; where it is
/// in the file is the caller's to show.
fn reading() -> serde_saphyr::Options {
    serde_saphyr::options! {
        strict_booleans: true,
        duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
        with_snippet: false,
    }
}

/// Parses `bytes`, which must be UTF-8, into `T`.
pub fn parse_bytes<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, YamlError> {
    let text = std::str::from_utf8(bytes).map_err(|source| YamlError::Utf8 { source })?;
    parse(text)
}

/// Parses an already-read [`Value`] into `T`. The error's path is
/// relative to `value`: a custom `Deserialize` that buffers a subtree
/// into a `Value` prefixes it with the subtree's own location.
pub fn from_value<T: DeserializeOwned>(value: Value) -> Result<T, YamlError> {
    serde_path_to_error::deserialize(value).map_err(|error| {
        YamlError::from(ParseRefusal {
            path: error.path().to_string(),
            message: error.into_inner().to_string(),
            at: None,
            nearest: None,
        })
    })
}

/// Serializes `value` as a YAML document.
pub fn to_string<T: Serialize>(value: &T) -> Result<String, YamlError> {
    // A line of text stays one line, however long: what the engine writes
    // is read back by a person and by a session, and a folded line is a
    // line neither wrote. Text that has lines of its own keeps them in a
    // literal block.
    let writing = serde_saphyr::ser_options! { folded_wrap_chars: usize::MAX };
    serde_saphyr::to_string_with_options(value, writing).map_err(|error| YamlError::Serialize {
        message: error.to_string(),
    })
}

/// `value` as a [`Value`], for a caller that rearranges a document
/// before writing it: the same tree [`parse`] reads back.
pub fn to_value<T: Serialize>(value: &T) -> Result<Value, YamlError> {
    parse(&to_string(value)?)
}
