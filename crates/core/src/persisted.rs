//! What this system writes down and reads again: a document with its
//! own version, and whatever a newer writer put beside it.
//!
//! Everything else this repository parses is authored — a workflow, a
//! config, a document a session hands over — and an authored file that
//! carries a key nobody declared is a mistake to name, not a field to
//! keep. A persisted file is the other direction: this binary wrote it,
//! a later one may rewrite it, and an older reader has to make sense of
//! what it finds. So a persisted document is tolerant by construction —
//! it keeps what it did not understand rather than dropping it, names
//! it when a reader asks, and refuses only what its own version says it
//! cannot read.
//!
//! One place, because the alternative is five files each deciding for
//! itself: today the manifest carries a `schema_version` nobody
//! compares, and the lock, the process registry, the isolation lock and
//! the receipt carry none at all.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

/// A document this system writes down and reads again.
pub trait Persisted: Serialize + DeserializeOwned {
    /// What this binary writes, and the highest it can read. A file
    /// stamped higher was written by a binary that knows something this
    /// one does not, and reading it would be guessing.
    const SCHEMA_VERSION: u32;
    /// How the document names itself to a reader — what a diagnostic
    /// about it says it is.
    const NAME: &'static str;
    /// Where its version is stamped, so the same field a reader compares
    /// is the one a writer wrote.
    const VERSION_KEY: &'static str = VERSION_KEY_DEFAULT;
    /// How the document is written down. Reading is one door for both —
    /// YAML reads JSON — but what a file is written as is what its name
    /// promises whoever opens it.
    const ENCODING: Encoding = Encoding::Yaml;
}

/// What a persisted document is written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Yaml,
    Json,
}

/// One persisted document as this binary read it: the version it was
/// stamped with, what this binary understood, and what it did not.
#[derive(Debug, Clone, PartialEq)]
pub struct PersistedDoc<T> {
    pub schema_version: u32,
    pub doc: T,
    /// Every top-level key this binary's own type does not declare,
    /// kept as it was written.
    ///
    /// Not dropped, because dropping is how a rewrite by an older binary
    /// silently deletes what a newer one recorded; and not an error,
    /// because a document of a version this one can read is a document
    /// it can act on. A reader that wants to say what it did not
    /// understand asks [`unknown_keys`](Self::unknown_keys).
    pub unknown: BTreeMap<String, serde_json::Value>,
}

/// Why a persisted document did not read.
#[derive(Debug, Error)]
pub enum PersistedError {
    #[error(
        "this `{name}` was written under schema {found}, and this binary reads up to \
         {supported} — upgrade yunta to read it"
    )]
    NewerWriter {
        name: &'static str,
        found: u32,
        supported: u32,
    },
    #[error("this `{name}` is not a `{name}`")]
    Unreadable {
        name: &'static str,
        #[source]
        cause: crate::yaml::YamlError,
    },
}

impl<T: Persisted> PersistedDoc<T> {
    /// A fresh document of this binary's own version.
    pub fn of(doc: T) -> Self {
        PersistedDoc {
            schema_version: T::SCHEMA_VERSION,
            doc,
            unknown: BTreeMap::new(),
        }
    }

    /// `bytes` read as this document, keeping what this binary does not
    /// understand.
    ///
    /// The version is read first and on its own: a file from a newer
    /// writer is refused naming both numbers, rather than parsed into a
    /// shape that happens to fit and acted on as if it were whole.
    pub fn read(bytes: &[u8]) -> Result<Self, PersistedError> {
        let unreadable = |cause| PersistedError::Unreadable {
            name: T::NAME,
            cause,
        };
        let value: serde_json::Value = crate::yaml::parse_bytes(bytes).map_err(unreadable)?;
        let schema_version = value
            .get(T::VERSION_KEY)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;
        if schema_version > T::SCHEMA_VERSION {
            return Err(PersistedError::NewerWriter {
                name: T::NAME,
                found: schema_version,
                supported: T::SCHEMA_VERSION,
            });
        }
        let doc: T = serde_json::from_value(value.clone()).map_err(|error| {
            unreadable(crate::yaml::YamlError::Parse {
                path: String::new(),
                message: error.to_string(),
            })
        })?;
        Ok(PersistedDoc {
            schema_version,
            unknown: unknown_beside(&value, &doc),
            doc,
        })
    }

    /// The bytes this binary writes for the document: its own version,
    /// its own fields, and whatever it kept from a newer writer.
    ///
    /// What was kept is written back, so an older binary rewriting a
    /// file a newer one wrote does not quietly delete what the newer one
    /// recorded.
    pub fn write(&self) -> Result<Vec<u8>, PersistedError> {
        let unreadable = |cause| PersistedError::Unreadable {
            name: T::NAME,
            cause,
        };
        let mut value = serde_json::to_value(&self.doc).map_err(|error| {
            unreadable(crate::yaml::YamlError::Serialize {
                message: error.to_string(),
            })
        })?;
        if let Some(object) = value.as_object_mut() {
            object.insert(
                T::VERSION_KEY.to_string(),
                serde_json::Value::from(T::SCHEMA_VERSION),
            );
            for (key, kept) in &self.unknown {
                object.entry(key.clone()).or_insert_with(|| kept.clone());
            }
        }
        let text = match T::ENCODING {
            Encoding::Yaml => crate::yaml::to_string(&value).map_err(unreadable)?,
            Encoding::Json => serde_json::to_string_pretty(&value).map_err(|error| {
                unreadable(crate::yaml::YamlError::Serialize {
                    message: error.to_string(),
                })
            })?,
        };
        Ok(text.into_bytes())
    }

    /// What this binary did not understand, as a reader names it — empty
    /// when the file held nothing beyond what this binary knows.
    pub fn unknown_keys(&self) -> Vec<&str> {
        self.unknown.keys().map(String::as_str).collect()
    }
}

/// Every top-level key in `written` that `read_back` does not account
/// for: what this binary's own type did not declare.
const VERSION_KEY_DEFAULT: &str = "schema_version";

fn unknown_beside<T: Serialize>(
    written: &serde_json::Value,
    read_back: &T,
) -> BTreeMap<String, serde_json::Value> {
    let (Some(written), Ok(serde_json::Value::Object(known))) =
        (written.as_object(), serde_json::to_value(read_back))
    else {
        return BTreeMap::new();
    };
    written
        .iter()
        .filter(|(key, _)| key.as_str() != VERSION_KEY_DEFAULT && !known.contains_key(*key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}
