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

    /// What a value written under a vocabulary this binary retired reads
    /// as now, applied before the document takes its own shape.
    ///
    /// Author input is refused when it uses a retired word — that is
    /// what "parse is validate" means on the way in. A persisted file is
    /// the other direction: this binary wrote that word itself, so the
    /// file is read under its replacement rather than refused. Documents
    /// that have retired nothing leave this alone.
    fn reconcile(_value: &mut crate::yaml::Value) {}

    /// JSON documents keep their existing JSON value path.
    fn reconcile_json(_value: &mut serde_json::Value) {}
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
        match T::ENCODING {
            Encoding::Yaml => Self::read_yaml(bytes),
            Encoding::Json => Self::read_json(bytes),
        }
    }

    fn check_version(schema_version: u32) -> Result<u32, PersistedError> {
        if schema_version > T::SCHEMA_VERSION {
            return Err(PersistedError::NewerWriter {
                name: T::NAME,
                found: schema_version,
                supported: T::SCHEMA_VERSION,
            });
        }
        Ok(schema_version)
    }

    fn read_yaml(bytes: &[u8]) -> Result<Self, PersistedError> {
        let value: crate::yaml::Value =
            crate::yaml::parse_bytes(bytes).map_err(Self::unreadable)?;
        let schema_version = Self::check_version(
            value
                .get(T::VERSION_KEY)
                .and_then(crate::yaml::Value::as_u64)
                .unwrap_or(0) as u32,
        )?;
        let mut reconciled = value.clone();
        T::reconcile(&mut reconciled);
        let doc = crate::yaml::from_value(reconciled).map_err(Self::unreadable)?;
        let json = serde_json::to_value(value).map_err(|error| {
            Self::unreadable(crate::yaml::YamlError::Serialize {
                message: error.to_string(),
            })
        })?;
        Ok(PersistedDoc {
            schema_version,
            unknown: unknown_beside(&json, &doc),
            doc,
        })
    }

    fn read_json(bytes: &[u8]) -> Result<Self, PersistedError> {
        let mut value: serde_json::Value =
            crate::yaml::parse_bytes(bytes).map_err(Self::unreadable)?;
        let schema_version = Self::check_version(
            value
                .get(T::VERSION_KEY)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u32,
        )?;
        T::reconcile_json(&mut value);
        let doc = serde_json::from_value(value.clone()).map_err(|error| {
            Self::unreadable(crate::yaml::YamlError::Parse {
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
        let text = match T::ENCODING {
            Encoding::Yaml => self.write_yaml()?,
            Encoding::Json => self.write_json()?,
        };
        Ok(text.into_bytes())
    }

    fn write_yaml(&self) -> Result<String, PersistedError> {
        // YAML mappings preserve insertion order. Passing the document
        // through JSON would alphabetize `modes:` and change promotion.
        let mut ordered = serde_norway::to_value(&self.doc).map_err(|error| {
            Self::unreadable(crate::yaml::YamlError::Serialize {
                message: error.to_string(),
            })
        })?;
        if let Some(fields) = ordered.as_mapping_mut() {
            fields.insert(
                crate::yaml::Value::String(T::VERSION_KEY.to_string()),
                crate::yaml::Value::from(T::SCHEMA_VERSION),
            );
            for (key, kept) in &self.unknown {
                let key = crate::yaml::Value::String(key.clone());
                if !fields.contains_key(&key) {
                    let kept = serde_norway::to_value(kept).map_err(|error| {
                        Self::unreadable(crate::yaml::YamlError::Serialize {
                            message: error.to_string(),
                        })
                    })?;
                    fields.insert(key, kept);
                }
            }
        }
        crate::yaml::to_string(&ordered).map_err(Self::unreadable)
    }

    fn write_json(&self) -> Result<String, PersistedError> {
        let mut value = serde_json::to_value(&self.doc).map_err(|error| {
            Self::unreadable(crate::yaml::YamlError::Serialize {
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
        serde_json::to_string_pretty(&value).map_err(|error| {
            Self::unreadable(crate::yaml::YamlError::Serialize {
                message: error.to_string(),
            })
        })
    }

    fn unreadable(cause: crate::yaml::YamlError) -> PersistedError {
        PersistedError::Unreadable {
            name: T::NAME,
            cause,
        }
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
