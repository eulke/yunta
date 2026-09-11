//! The vocabulary a document is walked with once serde has refused it.
//!
//! Every method reports and, where it has a value to return, returns
//! `None` — so a walk stops descending into something it could not make
//! sense of while still having said why. None of them knows any
//! particular document: what a task or a finding is lives with that
//! type, and this is only what the walks share.

use crate::diagnostic::{Diagnostic, Problem, Subject, ValueShape};
use crate::yaml::{Mapping, Value};

/// One key a writer plausibly reaches for: the key they wrote, the key
/// this schema spells it with (empty when the concept has no home here),
/// and the sentence that says so.
pub type Hint = (&'static str, &'static str, &'static str);

/// Collects what is wrong with a document that did not deserialize.
///
/// It carries the entry being walked, so no call has to repeat it and
/// no call can name a different one by accident. `at` moves to the next
/// entry; everything reported until then belongs to it.
pub struct Walk {
    diagnostics: Vec<Diagnostic>,
    subject: Subject,
}

impl Walk {
    pub(super) fn new() -> Self {
        Walk {
            diagnostics: Vec::new(),
            subject: Subject::Document,
        }
    }

    pub(super) fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    /// Names the entry every later call blames, until the next `at`.
    pub fn at(&mut self, subject: Subject) -> &mut Self {
        self.subject = subject;
        self
    }

    /// The entry currently being walked, for a caller that has to build
    /// a subject relative to it.
    pub fn subject(&self) -> &Subject {
        &self.subject
    }

    pub fn report(&mut self, problem: Problem) -> &mut Self {
        self.diagnostics
            .push(Diagnostic::new(self.subject.clone(), problem));
        self
    }

    /// The value as a mapping, or a diagnostic saying what was written
    /// instead and a line to copy.
    pub fn mapping<'v>(
        &mut self,
        value: &'v Value,
        expected: &str,
        example: &str,
    ) -> Option<&'v Mapping> {
        match value {
            Value::Mapping(mapping) => Some(mapping),
            other => {
                self.report(Problem::wrong_shape(shape_of(other), expected, example));
                None
            }
        }
    }

    /// The value as a list, or a diagnostic saying what was written
    /// instead and a line to copy.
    pub fn sequence<'v>(
        &mut self,
        value: &'v Value,
        expected: &str,
        example: &str,
    ) -> Option<&'v Vec<Value>> {
        match value {
            Value::Sequence(items) => Some(items),
            other => {
                self.report(Problem::wrong_shape(shape_of(other), expected, example));
                None
            }
        }
    }

    /// Reports every key the type does not accept and every required key
    /// the document left out.
    ///
    /// A key a hint redirects is reported once, not also as the absence
    /// it caused: telling a writer that `question` is unknown AND that
    /// `text` is missing describes one mistake as two, and a reader
    /// correcting a list of problems has no way to tell that the second
    /// disappears with the first.
    pub fn keys(
        &mut self,
        mapping: &Mapping,
        required: &[&str],
        optional: &[&str],
        hints: &[Hint],
    ) -> &mut Self {
        let valid: Vec<&str> = required.iter().chain(optional).copied().collect();
        let mut already_explained: Vec<&str> = Vec::new();
        for (key, _) in mapping {
            let Some(name) = key.as_str() else {
                self.report(Problem::wrong_shape(
                    shape_of(key),
                    "a key",
                    valid.first().copied().unwrap_or(""),
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
            self.report(problem);
        }
        for key in required {
            if mapping.get(*key).is_none() && !already_explained.contains(key) {
                self.report(Problem::missing_key(*key));
            }
        }
        self
    }

    /// Reports a value that should have been text and was not.
    pub fn string(&mut self, mapping: &Mapping, key: &str, example: &str) -> &mut Self {
        if let Some(value) = mapping.get(key) {
            if !matches!(value, Value::String(_)) {
                self.report(Problem::wrong_shape(shape_of(value), "text", example));
            }
        }
        self
    }

    /// Reports a value that should have been a list of text and was not,
    /// naming every item that is not text rather than only the first.
    pub fn string_list(
        &mut self,
        mapping: &Mapping,
        key: &str,
        expected: &str,
        example: &str,
    ) -> &mut Self {
        let Some(value) = mapping.get(key) else {
            return self;
        };
        let Some(items) = self.sequence(value, expected, example) else {
            return self;
        };
        let wrong: Vec<ValueShape> = items
            .iter()
            .filter(|item| !matches!(item, Value::String(_)))
            .map(shape_of)
            .collect();
        for found in wrong {
            self.report(Problem::wrong_shape(found, "text", example));
        }
        self
    }

    /// Reports a value outside the closed set its key accepts.
    pub fn one_of(
        &mut self,
        mapping: &Mapping,
        key: &str,
        valid: &[&str],
        example: &str,
    ) -> &mut Self {
        let Some(value) = mapping.get(key) else {
            return self;
        };
        match value.as_str() {
            Some(text) if valid.contains(&text) => self,
            Some(text) => self.report(Problem::unknown_value(text, valid.iter().copied())),
            None => self.report(Problem::wrong_shape(shape_of(value), "text", example)),
        }
    }

    /// Reports a value that should have been `true` or `false`.
    pub fn boolean(&mut self, mapping: &Mapping, key: &str, example: &str) -> &mut Self {
        if let Some(value) = mapping.get(key) {
            if !matches!(value, Value::Bool(_)) {
                self.report(Problem::wrong_shape(
                    shape_of(value),
                    "true or false",
                    example,
                ));
            }
        }
        self
    }

    /// The entry's declared id when it is readable.
    ///
    /// An unreadable id is exactly the case that makes a deserializer's
    /// path useless, so the entry falls back to naming itself by
    /// position and the walk keeps going.
    pub fn id<T: std::str::FromStr<Err = crate::InvalidId>>(
        &mut self,
        mapping: &Mapping,
    ) -> Option<T> {
        let raw = mapping.get("id")?;
        match raw.as_str() {
            Some(text) => match text.parse::<T>() {
                Ok(id) => Some(id),
                Err(invalid) => {
                    self.report(Problem::invalid_id(invalid.value, invalid.rule));
                    None
                }
            },
            None => {
                self.report(Problem::wrong_shape(
                    shape_of(raw),
                    "text",
                    "id: add-dark-mode",
                ));
                None
            }
        }
    }

    /// The one-entry document every interpreted artifact is: a single
    /// top-level key holding a list. `entry` is called once per item,
    /// with the walk it reports into.
    pub fn entries(
        &mut self,
        value: &Value,
        key: &'static str,
        example: &'static str,
        mut entry: impl FnMut(&mut Walk, usize, &Value),
    ) {
        self.at(Subject::Document);
        let expected = format!("a mapping with `{key}:`");
        let Some(root) = self.mapping(value, &expected, example) else {
            return;
        };
        self.keys(root, &[key], &[], &[]);
        let Some(list) = root.get(key) else {
            return;
        };
        let expected = format!("a list under `{key}:`");
        let Some(items) = self.sequence(list, &expected, example) else {
            return;
        };
        for (index, item) in items.iter().enumerate() {
            entry(self, index, item);
        }
    }
}

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
