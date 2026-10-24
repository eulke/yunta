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

use surco::text::nearest;

use super::keys::{node_keys, retired, WORKFLOW_KEYS};
use super::WorkflowFault;
use crate::diagnostic::{Diagnostic, Named, Problem, Subject};
use crate::yaml::{Mapping, Pointer, ReadAs, SourceMap, Value};
use crate::NodeKind;

/// What the audit found, and the document it hands on.
pub(super) struct Audit {
    pub(super) broken: Vec<Diagnostic>,
    pub(super) repaired: Value,
    /// Each key read as the one it meant: where what is said about the
    /// key meant is written.
    pub(super) read_as: Vec<ReadAs>,
}

/// Every key `document`, read from `text`, writes that nothing reads.
pub(super) fn audit(document: Value, text: &str) -> Audit {
    let mut auditor = Auditor {
        text,
        map: None,
        broken: Vec::new(),
        read_as: Vec::new(),
        seen: 0,
    };
    let repaired = match document {
        Value::Mapping(mapping) => Value::Mapping(auditor.workflow(mapping)),
        other => other,
    };
    Audit {
        broken: auditor.broken,
        repaired,
        read_as: auditor.read_as,
    }
}

struct Auditor<'a> {
    text: &'a str,
    /// Read only once there is something to place.
    map: Option<SourceMap>,
    broken: Vec<Diagnostic>,
    read_as: Vec<ReadAs>,
    /// How many nodes the audit has met, in the order the document has
    /// them: what names a node whose id cannot be read.
    seen: usize,
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
                        Value::Sequence(self.nodes(nodes, Pointer::root().key("nodes")))
                    }
                    (_, value) => value,
                };
                kept.insert(key, value);
                continue;
            }
            let fault = WorkflowFault::UnknownKey {
                key: name.clone(),
                kind: None,
                nearest: nearest(&name, WORKFLOW_KEYS.iter().copied()).map(str::to_string),
            };
            self.refuse(
                &Pointer::root().key(name.as_str()),
                Subject::Document,
                fault,
            );
            if let Some(meant) = self.meant(&name, WORKFLOW_KEYS, &present) {
                self.read(Pointer::root(), name, meant);
                kept.insert(Value::from(meant), value);
            }
        }
        kept
    }

    /// The nodes of a `nodes:` list at `at`, each audited.
    fn nodes(&mut self, nodes: Vec<Value>, at: Pointer) -> Vec<Value> {
        nodes
            .into_iter()
            .enumerate()
            .map(|(index, node)| match node {
                Value::Mapping(mapping) => {
                    Value::Mapping(self.node(mapping, at.clone().index(index)))
                }
                other => other,
            })
            .collect()
    }

    /// One node's keys, judged against its own and its kind's. A node
    /// whose kind is not one, and is no slip from one, is left to the
    /// parser, which names the kinds there are: which keys belong to it is
    /// not known.
    fn node(&mut self, mut mapping: Mapping, at: Pointer) -> Mapping {
        let id = mapping
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| id.parse().ok());
        let subject = Subject::Node(Named::new(id, self.seen));
        self.seen += 1;
        let Some(kind) = self.kind_of(&mut mapping, &at, &subject) else {
            return mapping;
        };
        let Some(valid) = node_keys(&kind) else {
            return mapping;
        };
        let present = keys(&mapping);
        let mut kept = Mapping::new();
        for (key, value) in mapping {
            let Some(name) = key.as_str().map(str::to_string) else {
                kept.insert(key, value);
                continue;
            };
            if valid.contains(&name.as_str()) {
                let value = match (kind.as_str(), name.as_str(), value) {
                    ("parallel", "nodes", Value::Sequence(children)) => {
                        Value::Sequence(self.nodes(children, at.clone().key("nodes")))
                    }
                    (_, _, value) => value,
                };
                kept.insert(key, value);
                continue;
            }
            let fault = unknown_node_key(&name, &kind, &valid);
            self.refuse(&at.clone().key(name.as_str()), subject.clone(), fault);
            if let Some(meant) = self.meant(&name, &valid, &present) {
                self.read(at.clone(), name, meant);
                kept.insert(Value::from(meant), value);
            }
        }
        kept
    }

    /// The kind `mapping` is: the one it writes, or the one a slip in it
    /// most likely meant, refused where it is written and read as meant.
    /// `None` when it writes no kind, or one nothing is a slip from.
    fn kind_of(
        &mut self,
        mapping: &mut Mapping,
        at: &Pointer,
        subject: &Subject,
    ) -> Option<String> {
        let written = mapping
            .get(NodeKind::TAG)
            .and_then(Value::as_str)?
            .to_string();
        if NodeKind::keys(&written).is_some() {
            return Some(written);
        }
        let near = nearest(&written, NodeKind::KINDS.iter().copied())?;
        let map = self.map.get_or_insert_with(|| SourceMap::read(self.text));
        let place = map.place(&at.clone().key(NodeKind::TAG));
        let fault = WorkflowFault::UnknownKind {
            kind: written,
            nearest: near.to_string(),
        };
        self.broken
            .push(Diagnostic::new(subject.clone(), Problem::broke(fault)).at(place));
        mapping.insert(Value::from(NodeKind::TAG), Value::from(near));
        Some(near.to_string())
    }

    /// The key `typed` most likely meant, when it is unambiguous and the
    /// mapping does not write that key already — the one case where
    /// reading the value under it is reading what the person meant.
    fn meant<'v>(&self, typed: &str, valid: &[&'v str], present: &[String]) -> Option<&'v str> {
        nearest(typed, valid.iter().copied())
            .filter(|meant| !present.iter().any(|key| key == meant))
    }

    /// Records that the mapping at `at` writes `written` and is read as
    /// writing `meant`.
    fn read(&mut self, at: Pointer, written: String, meant: &str) {
        self.read_as.push(ReadAs {
            at,
            written,
            meant: meant.to_string(),
        });
    }

    /// Records that the key at `at`, written about `subject`, is read by
    /// nothing.
    fn refuse(&mut self, at: &Pointer, subject: Subject, fault: WorkflowFault) {
        let map = self.map.get_or_insert_with(|| SourceMap::read(self.text));
        let place = map.locate_key(at);
        self.broken
            .push(Diagnostic::new(subject, Problem::broke(fault)).at(place));
    }
}

/// The refusal of `name`, a key no `kind` node reads, with the key a typo
/// is one slip from; a key retired is told what took its place instead.
fn unknown_node_key(name: &str, kind: &str, valid: &[&str]) -> WorkflowFault {
    let near = match retired(name) {
        Some(_) => None,
        None => nearest(name, valid.iter().copied()).map(str::to_string),
    };
    WorkflowFault::UnknownKey {
        key: name.to_string(),
        kind: Some(kind.to_string()),
        nearest: near,
    }
}

/// The keys `mapping` writes, as text.
fn keys(mapping: &Mapping) -> Vec<String> {
    mapping
        .keys()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}
