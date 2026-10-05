//! The keys a workflow writes that nothing reads, every one at once.
//!
//! The parser stops at the first key it does not know, and a person who
//! typed three wrong hears about one of them, fixes it, and hears about
//! the next. The audit reads the document's keys before the parser
//! does — at the top, in each node, in each child of a `parallel` group
//! — and reports every unknown one where it is written, with the key it
//! most likely meant. What it hands on is the document with those keys
//! taken out, or renamed to the key meant when that key is missing, so
//! that what the document says next is judged as the person meant it.

use crate::diagnostic::{Diagnostic, Problem, Subject};
use crate::text::{did_you_mean, nearest};
use crate::yaml::{Mapping, Pointer, SourceMap, Value};
use crate::NodeKind;

use super::node::{NODE_KEYS, RETIRED_NODE_KEYS};
use super::parse::list;

/// The keys a workflow accepts at its top.
pub(super) const WORKFLOW_KEYS: &[&str] = &[
    "name",
    "description",
    "modes",
    "inputs",
    "node_defaults",
    "nodes",
    "yunta_schema",
    "on_finish",
];

/// What the audit found, and the document it hands on.
pub(super) struct Audit {
    pub(super) broken: Vec<Diagnostic>,
    pub(super) repaired: Value,
}

/// Every key `document`, read from `text`, writes that nothing reads.
pub(super) fn audit(document: Value, text: &str) -> Audit {
    let mut auditor = Auditor {
        text,
        map: None,
        broken: Vec::new(),
    };
    let repaired = match document {
        Value::Mapping(mapping) => Value::Mapping(auditor.workflow(mapping)),
        other => other,
    };
    Audit {
        broken: auditor.broken,
        repaired,
    }
}

struct Auditor<'a> {
    text: &'a str,
    /// Read only once there is something to place.
    map: Option<SourceMap>,
    broken: Vec<Diagnostic>,
}

impl Auditor<'_> {
    fn workflow(&mut self, mapping: Mapping) -> Mapping {
        let mut kept = Mapping::new();
        let present: Vec<String> = keys(&mapping);
        for (key, value) in mapping {
            let Some(name) = key.as_str().map(str::to_string) else {
                kept.insert(key, value);
                continue;
            };
            if WORKFLOW_KEYS.contains(&name.as_str()) {
                let value = match (name.as_str(), value) {
                    ("nodes", Value::Sequence(nodes)) => {
                        Value::Sequence(self.nodes(nodes, Pointer::root().key("nodes"), "nodes"))
                    }
                    (_, value) => value,
                };
                kept.insert(key, value);
                continue;
            }
            let said = format!(
                "unknown field `{name}`, expected one of {}{}",
                list(WORKFLOW_KEYS),
                did_you_mean(&name, WORKFLOW_KEYS.iter().copied())
            );
            self.refuse(&Pointer::root().key(name.as_str()), "", said);
            if let Some(meant) = self.meant(&name, WORKFLOW_KEYS, &present) {
                kept.insert(Value::from(meant), value);
            }
        }
        kept
    }

    /// The nodes of a `nodes:` list at `at`, each audited; `path` is how a
    /// deserializer names the list.
    fn nodes(&mut self, nodes: Vec<Value>, at: Pointer, path: &str) -> Vec<Value> {
        nodes
            .into_iter()
            .enumerate()
            .map(|(index, node)| match node {
                Value::Mapping(mapping) => Value::Mapping(self.node(
                    mapping,
                    at.clone().index(index),
                    &format!("{path}[{index}]"),
                )),
                other => other,
            })
            .collect()
    }

    /// One node's keys, judged against its own and its kind's. A node
    /// whose kind is not one is left to the parser, which names the kinds
    /// there are: which keys belong to it is not known.
    fn node(&mut self, mapping: Mapping, at: Pointer, path: &str) -> Mapping {
        let Some(kind_keys) = mapping
            .get("kind")
            .and_then(Value::as_str)
            .and_then(NodeKind::keys)
        else {
            return mapping;
        };
        let kind = mapping
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let subject = match mapping.get("id").and_then(Value::as_str) {
            Some(id) => format!("node `{id}`"),
            None => "node".to_string(),
        };
        let valid: Vec<&str> = NODE_KEYS
            .iter()
            .chain(std::iter::once(&"kind"))
            .chain(kind_keys.iter())
            .copied()
            .collect();
        let present = keys(&mapping);
        let mut kept = Mapping::new();
        for (key, value) in mapping {
            let Some(name) = key.as_str().map(str::to_string) else {
                kept.insert(key, value);
                continue;
            };
            if valid.contains(&name.as_str()) {
                let value = match (kind.as_str(), name.as_str(), value) {
                    ("parallel", "nodes", Value::Sequence(children)) => Value::Sequence(
                        self.nodes(children, at.clone().key("nodes"), &format!("{path}.nodes")),
                    ),
                    (_, _, value) => value,
                };
                kept.insert(key, value);
                continue;
            }
            let said = unknown_node_key(&subject, &name, &kind, &valid);
            self.refuse(&at.clone().key(name.as_str()), path, said);
            if let Some(meant) = self.meant(&name, &valid, &present) {
                kept.insert(Value::from(meant), value);
            }
        }
        kept
    }

    /// The key `typed` most likely meant, when it is unambiguous and the
    /// mapping does not write that key already — the one case where
    /// reading the value under it is reading what the person meant.
    fn meant<'v>(&self, typed: &str, valid: &[&'v str], present: &[String]) -> Option<&'v str> {
        nearest(typed, valid.iter().copied())
            .filter(|meant| !present.iter().any(|key| key == meant))
    }

    /// Records that the key at `at` is read by nothing.
    fn refuse(&mut self, at: &Pointer, path: &str, said: String) {
        let map = self.map.get_or_insert_with(|| SourceMap::read(self.text));
        let place = map.locate_key(at);
        self.broken
            .push(Diagnostic::new(Subject::Document, Problem::parse(path, said)).at(place));
    }
}

/// The refusal of `name`, a key no `kind` node reads: the keys it may
/// write, and the key that replaced a retired one or the key a typo is
/// one slip from.
fn unknown_node_key(subject: &str, name: &str, kind: &str, valid: &[&str]) -> String {
    let hint = RETIRED_NODE_KEYS
        .iter()
        .find(|(retired, _)| *retired == name)
        .map(|(_, hint)| format!("; {hint}"))
        .unwrap_or_else(|| did_you_mean(name, valid.iter().copied()));
    format!(
        "{subject}: unknown key `{name}` for a `{kind}` node; valid keys: {}{hint}",
        list(valid)
    )
}

/// The keys `mapping` writes, as text.
fn keys(mapping: &Mapping) -> Vec<String> {
    mapping
        .keys()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}
