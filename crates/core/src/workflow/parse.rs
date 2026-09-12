//! Deserialization shared by the key-discriminated types: reading the
//! one entry of a mapping, the value under it, and the wording every
//! error here uses to list keys and describe values.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

use crate::yaml::{self, Mapping, Value, YamlError};

/// Reads a mapping that holds exactly one entry whose key is one of
/// `keys` — the shape of every value this schema discriminates by a
/// field name. `what` names the value in the error. An entry written
/// under one of `aliases` comes back under the key it stands for; the
/// error lists `keys` alone, so an alias is read but never advertised.
pub(super) fn keyed_entry<'de, D: Deserializer<'de>>(
    deserializer: D,
    what: &str,
    keys: &[&str],
    aliases: &[(&str, &str)],
) -> Result<(String, Value), D::Error> {
    use serde::de::Error;

    let mut entries = Mapping::deserialize(deserializer)?.into_iter();
    let (key, value) = match (entries.next(), entries.next()) {
        (Some(entry), None) => entry,
        _ => {
            return Err(D::Error::custom(format!(
                "{what} is a mapping with exactly one key, one of {}",
                list(keys)
            )))
        }
    };
    let Some(key) = key.as_str() else {
        return Err(D::Error::custom(format!(
            "{what} is keyed by a string, one of {}",
            list(keys)
        )));
    };
    let key = aliases
        .iter()
        .find(|(alias, _)| *alias == key)
        .map_or(key, |(_, canonical)| canonical);
    if !keys.contains(&key) {
        return Err(D::Error::custom(format!(
            "unknown key `{key}` for {what}; one of {}",
            list(keys)
        )));
    }
    Ok((key.to_string(), value))
}

/// Parses the value found under `key`, keeping `key` in the error's
/// path so the location stays complete once the parser adds its own.
pub(super) fn nested<'de, D: Deserializer<'de>, T: DeserializeOwned>(
    key: &str,
    value: Value,
) -> Result<T, D::Error> {
    use serde::de::Error;

    yaml::from_value(value).map_err(|error| {
        D::Error::custom(match error {
            YamlError::Parse { path, message } if path.is_empty() || path == "." => {
                format!("{key}: {message}")
            }
            YamlError::Parse { path, message } => format!("{key}.{path}: {message}"),
            other => other.to_string(),
        })
    })
}

/// `` `a`, `b`, `c` `` — how every error here lists keys.
pub(super) fn list<S: AsRef<str>>(keys: &[S]) -> String {
    keys.iter()
        .map(|key| format!("`{}`", key.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What kind of YAML value `value` is, for an error that expected another.
pub(super) fn describe(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Sequence(_) => "a list",
        Value::Mapping(_) => "a mapping",
        Value::Tagged(_) => "a tagged value",
    }
}
