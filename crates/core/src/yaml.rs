//! The one place YAML enters and leaves the system.
//!
//! Every file a person or an agent writes — workflows, config layers,
//! pack manifests, test cases, mock fixtures, ledgers, findings,
//! questions — parses through [`parse`]; what the engine persists and
//! reads back (manifests, locks) parses through the same function with
//! types that tolerate what they do not know. A parse error names the
//! path of the value that failed (`nodes[2].artifacts.produces[0]`), so
//! a caller only ever adds the file.

use serde::de::DeserializeOwned;
use serde::Serialize;

pub use serde_norway::{Mapping, Value};

/// A YAML document that could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum YamlError {
    /// The document does not parse into the expected type. `path`
    /// locates the offending value from the document's root; it is
    /// empty when the root itself is the problem.
    #[error("{}", locate(path, message))]
    Parse { path: String, message: String },
    #[error("not valid UTF-8: {source}")]
    Utf8 {
        #[source]
        source: std::str::Utf8Error,
    },
    #[error("cannot serialize: {message}")]
    Serialize { message: String },
}

fn locate(path: &str, message: &str) -> String {
    if path.is_empty() || path == "." {
        message.to_string()
    } else {
        format!("`{path}`: {message}")
    }
}

/// Parses `text` into `T`. An error locates the failing value by its
/// path from the document's root, and keeps the parser's own line and
/// column.
pub fn parse<T: DeserializeOwned>(text: &str) -> Result<T, YamlError> {
    serde_path_to_error::deserialize(serde_norway::Deserializer::from_str(text)).map_err(|error| {
        YamlError::Parse {
            path: error.path().to_string(),
            message: error.into_inner().to_string(),
        }
    })
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
    serde_path_to_error::deserialize(value).map_err(|error| YamlError::Parse {
        path: error.path().to_string(),
        message: error.into_inner().to_string(),
    })
}

/// Serializes `value` as a YAML document.
pub fn to_string<T: Serialize>(value: &T) -> Result<String, YamlError> {
    serde_norway::to_string(value).map_err(|error| YamlError::Serialize {
        message: error.to_string(),
    })
}
