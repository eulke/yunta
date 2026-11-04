//! The keys of the entries a workflow's lists hold, judged as a node's are.

use surco::text::nearest;

use super::super::context::{ContextFile, ContextSpec};
use super::super::{ArtifactRefId, OnFinishStep, WorkflowFault};
use super::{keys, Auditor};
use crate::diagnostic::Subject;
use crate::yaml::{Mapping, Pointer, Value};

/// What a context source's `artifact:` reads, beside what names the artifact.
const CONTEXT_ARTIFACT: &[&str] = &["node"];

/// What a mount's `artifact:` reads, beside what names the artifact.
const MOUNT_ARTIFACT: &[&str] = &["node", "as"];

impl Auditor<'_> {
    /// The value under a key a node of `kind` accepts: a list it holds is judged entry by
    /// entry, as the node's own keys are, and any other value is kept as it is.
    pub(super) fn listed(
        &mut self,
        kind: &str,
        name: &str,
        value: Value,
        at: &Pointer,
        subject: &Subject,
    ) -> Value {
        match (kind, name, value) {
            ("parallel", "nodes", Value::Sequence(children)) => {
                Value::Sequence(self.nodes(children, at.clone().key("nodes")))
            }
            (_, "context", Value::Sequence(sources)) => {
                Value::Sequence(self.sources(sources, at.clone().key("context"), subject))
            }
            ("workflow", "mounts", Value::Sequence(mounts)) => {
                Value::Sequence(self.mounts(mounts, at.clone().key("mounts"), subject))
            }
            (_, _, value) => value,
        }
    }

    /// `mapping`'s keys against `valid`, an entry of the kind `what` says at `at`: each
    /// unknown one refused where it is written, and read as the key it meant when it is a
    /// slip from one the entry does not write.
    fn entry(
        &mut self,
        mapping: Mapping,
        at: &Pointer,
        subject: &Subject,
        what: &str,
        valid: &[&str],
    ) -> Mapping {
        let present = keys(&mapping);
        let mut kept = Mapping::new();
        for (key, value) in mapping {
            let Some(name) = key.as_str().map(str::to_string) else {
                kept.insert(key, value);
                continue;
            };
            if valid.contains(&name.as_str()) {
                kept.insert(key, value);
                continue;
            }
            let fault = WorkflowFault::UnknownEntryKey {
                key: name.clone(),
                what: what.to_string(),
                nearest: nearest(&name, valid.iter().copied()).map(str::to_string),
            };
            self.refuse(&at.clone().key(name.as_str()), subject.clone(), fault);
            if let Some(meant) = self.meant(&name, valid, &present) {
                self.read(at.clone(), name, meant);
                kept.insert(Value::from(meant), value);
            }
        }
        kept
    }

    /// A `context:` list: each source's own key, and what its `files:` and `artifact:` hold.
    /// A source that wrote keys and is left with none is taken out, so it is not refused a
    /// second time for that; one that wrote none is left for the parser to refuse.
    pub(super) fn sources(
        &mut self,
        sources: Vec<Value>,
        at: Pointer,
        subject: &Subject,
    ) -> Vec<Value> {
        let mut kept = Vec::new();
        for (index, source) in sources.into_iter().enumerate() {
            let Value::Mapping(mapping) = source else {
                kept.push(source);
                continue;
            };
            // A spelling an earlier workflow may carry is read, and never suggested.
            if keys(&mapping)
                .iter()
                .any(|key| ContextSpec::ALIASES.iter().any(|(alias, _)| alias == key))
            {
                kept.push(Value::Mapping(mapping));
                continue;
            }
            let at = at.clone().index(index);
            let written = !mapping.is_empty();
            let mut source =
                self.entry(mapping, &at, subject, "a context source", ContextSpec::KEYS);
            if written && source.is_empty() {
                continue;
            }
            if let Some(files) = source.get_mut("files") {
                *files = self.files(std::mem::take(files), at.clone().key("files"), subject);
            }
            if let Some(Value::Mapping(reference)) = source.get_mut("artifact") {
                *reference = self.reference(
                    std::mem::take(reference),
                    &at.clone().key("artifact"),
                    subject,
                    "an `artifact:` context source",
                    CONTEXT_ARTIFACT,
                );
            }
            kept.push(Value::Mapping(source));
        }
        kept
    }

    /// A `files:` source: a path is a path, and an entry written as a mapping has its keys.
    fn files(&mut self, files: Value, at: Pointer, subject: &Subject) -> Value {
        let Value::Sequence(files) = files else {
            return files;
        };
        Value::Sequence(
            files
                .into_iter()
                .enumerate()
                .map(|(index, file)| match file {
                    Value::Mapping(mapping) => Value::Mapping(self.entry(
                        mapping,
                        &at.clone().index(index),
                        subject,
                        "a `files:` entry",
                        ContextFile::KEYS,
                    )),
                    other => other,
                })
                .collect(),
        )
    }

    /// An artifact reference: `claimed`, what its container reads, then what names the artifact.
    fn reference(
        &mut self,
        reference: Mapping,
        at: &Pointer,
        subject: &Subject,
        what: &str,
        claimed: &[&str],
    ) -> Mapping {
        let valid: Vec<&str> = claimed.iter().copied().chain(ArtifactRefId::KEYS).collect();
        self.entry(reference, at, subject, what, &valid)
    }

    /// A `mounts:` list: each entry's `artifact:`.
    pub(super) fn mounts(
        &mut self,
        mounts: Vec<Value>,
        at: Pointer,
        subject: &Subject,
    ) -> Vec<Value> {
        mounts
            .into_iter()
            .enumerate()
            .map(|(index, mount)| {
                let Value::Mapping(mut mapping) = mount else {
                    return mount;
                };
                if let Some(Value::Mapping(reference)) = mapping.get_mut("artifact") {
                    *reference = self.reference(
                        std::mem::take(reference),
                        &at.clone().index(index).key("artifact"),
                        subject,
                        "a `mounts:` entry",
                        MOUNT_ARTIFACT,
                    );
                }
                Value::Mapping(mapping)
            })
            .collect()
    }

    /// An `on_finish:` list: each step's own key. A step that wrote keys and is left with
    /// none is taken out; one that wrote none is left for the parser to refuse.
    pub(super) fn closing_steps(&mut self, steps: Vec<Value>, at: Pointer) -> Vec<Value> {
        let mut kept = Vec::new();
        for (index, step) in steps.into_iter().enumerate() {
            let Value::Mapping(mapping) = step else {
                kept.push(step);
                continue;
            };
            let written = !mapping.is_empty();
            let step = self.entry(
                mapping,
                &at.clone().index(index),
                &Subject::Document,
                "an `on_finish` step",
                OnFinishStep::KEYS,
            );
            if !written || !step.is_empty() {
                kept.push(Value::Mapping(step));
            }
        }
        kept
    }
}
