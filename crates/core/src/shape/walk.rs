//! How a document is walked once serde has refused it.
//!
//! Each helper reports and returns `None`, so a walk stops descending
//! into a value it could not make sense of while still having said why.
//! None of them knows any particular document: what a task or a finding
//! is lives in `documents.rs`, and this is only the vocabulary the two
//! share.

use crate::diagnostic::{Diagnostic, Problem, Subject, ValueShape};
use crate::yaml::{Mapping, Value};

pub(super) fn shape_of(value: &Value) -> ValueShape {
    match value {
        Value::Null => ValueShape::Null,
        Value::Bool(_) => ValueShape::Bool,
        Value::Number(_) => ValueShape::Number,
        Value::String(_) => ValueShape::String,
        Value::Sequence(_) => ValueShape::Sequence,
        Value::Mapping(_) => ValueShape::Mapping,
        Value::Tagged(_) => ValueShape::Tagged,
    }
}

pub(super) fn as_mapping<'a>(
    value: &'a Value,
    subject: &Subject,
    expected: &str,
    example: &str,
    into: &mut Vec<Diagnostic>,
) -> Option<&'a Mapping> {
    match value {
        Value::Mapping(mapping) => Some(mapping),
        other => {
            into.push(Diagnostic::new(
                subject.clone(),
                Problem::wrong_shape(shape_of(other), expected, example),
            ));
            None
        }
    }
}

pub(super) fn as_sequence<'a>(
    value: &'a Value,
    subject: &Subject,
    expected: &str,
    example: &str,
    into: &mut Vec<Diagnostic>,
) -> Option<&'a Vec<Value>> {
    match value {
        Value::Sequence(items) => Some(items),
        other => {
            into.push(Diagnostic::new(
                subject.clone(),
                Problem::wrong_shape(shape_of(other), expected, example),
            ));
            None
        }
    }
}

/// One key a writer plausibly reaches for: the key they wrote, the key
/// this schema spells it with (empty when the concept has no home here),
/// and the sentence that says so.
pub(super) type Hint = (&'static str, &'static str, &'static str);

/// Reports every key the type does not accept and every required key the
/// document left out.
///
/// A key a hint redirects is reported once, not also as the absence it
/// caused: telling a writer that `question` is unknown AND that `text` is
/// missing describes one mistake as two, and a reader correcting a list
/// of problems has no way to tell that the second disappears with the
/// first.
pub(super) fn check_keys(
    mapping: &Mapping,
    subject: &Subject,
    required: &[&str],
    optional: &[&str],
    hints: &[Hint],
    into: &mut Vec<Diagnostic>,
) {
    let valid: Vec<&str> = required.iter().chain(optional).copied().collect();
    let mut already_explained: Vec<&str> = Vec::new();
    for (key, _) in mapping {
        let Some(name) = key.as_str() else {
            into.push(Diagnostic::new(
                subject.clone(),
                Problem::wrong_shape(shape_of(key), "a key", valid.first().copied().unwrap_or("")),
            ));
            continue;
        };
        if valid.contains(&name) {
            continue;
        }
        let problem = match hints.iter().find(|(bad, ..)| *bad == name) {
            Some((_, replacement, hint)) => {
                if !replacement.is_empty() {
                    already_explained.push(replacement);
                }
                Problem::unknown_key_instead(name, valid.iter().copied(), *hint)
            }
            None => Problem::unknown_key(name, valid.iter().copied()),
        };
        into.push(Diagnostic::new(subject.clone(), problem));
    }
    for key in required {
        if mapping.get(*key).is_none() && !already_explained.contains(key) {
            into.push(Diagnostic::new(subject.clone(), Problem::missing_key(*key)));
        }
    }
}

/// Reports a value that should have been text and was not.
pub(super) fn check_string(
    mapping: &Mapping,
    key: &str,
    subject: &Subject,
    example: &str,
    into: &mut Vec<Diagnostic>,
) {
    if let Some(value) = mapping.get(key) {
        if !matches!(value, Value::String(_)) {
            into.push(Diagnostic::new(
                subject.clone(),
                Problem::wrong_shape(shape_of(value), "text", example),
            ));
        }
    }
}

/// Reports a list of globs that is not a list of text.
pub(super) fn check_string_list(
    mapping: &Mapping,
    key: &str,
    subject: &Subject,
    expected: &str,
    example: &str,
    into: &mut Vec<Diagnostic>,
) {
    let Some(value) = mapping.get(key) else {
        return;
    };
    let Some(items) = as_sequence(value, subject, expected, example, into) else {
        return;
    };
    for item in items {
        if !matches!(item, Value::String(_)) {
            into.push(Diagnostic::new(
                subject.clone(),
                Problem::wrong_shape(shape_of(item), "text", example),
            ));
        }
    }
}

/// The identifier a document entry declares, when it is readable, plus
/// the diagnostic when it is not. An unreadable id is exactly the case
/// that makes a deserializer's path useless, so the entry falls back to
/// naming itself by position.
pub(super) fn read_id<T: std::str::FromStr<Err = crate::InvalidId>>(
    mapping: &Mapping,
    subject: &Subject,
    into: &mut Vec<Diagnostic>,
) -> Option<T> {
    let raw = mapping.get("id")?;
    match raw.as_str() {
        Some(text) => match text.parse::<T>() {
            Ok(id) => Some(id),
            Err(invalid) => {
                into.push(Diagnostic::new(
                    subject.clone(),
                    Problem::invalid_id(invalid.value, invalid.rule),
                ));
                None
            }
        },
        None => {
            into.push(Diagnostic::new(
                subject.clone(),
                Problem::wrong_shape(shape_of(raw), "text", "id: add-dark-mode"),
            ));
            None
        }
    }
}

/// The one-entry document every interpreted artifact is: a single
/// top-level key holding a list.
pub(super) fn walk_entries(
    value: &Value,
    key: &'static str,
    example: &'static str,
    into: &mut Vec<Diagnostic>,
    mut entry: impl FnMut(usize, &Value, &mut Vec<Diagnostic>),
) {
    let document = Subject::Document;
    let Some(root) = as_mapping(
        value,
        &document,
        &format!("a mapping with `{key}:`"),
        example,
        into,
    ) else {
        return;
    };
    check_keys(root, &document, &[key], &[], &[], into);
    let Some(list) = root.get(key) else {
        return;
    };
    let Some(items) = as_sequence(
        list,
        &document,
        &format!("a list under `{key}:`"),
        example,
        into,
    ) else {
        return;
    };
    for (index, item) in items.iter().enumerate() {
        entry(index, item, into);
    }
}
