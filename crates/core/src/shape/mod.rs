//! Reading a document an agent wrote, and publishing the shape it
//! should have written.
//!
//! [`read`] parses with serde on the happy path, so the schema is
//! declared exactly once — in the types — and never restated here. Only
//! when serde refuses does the document get walked a second time, by
//! [`Document::diagnose`], whose single job is to name every problem at
//! once in the vocabulary of the document. A reader that corrects one
//! problem per round pays a round per problem; a repair cycle that works
//! that way exhausts its budget before the file is readable.
//!
//! [`read`] then runs [`Document::check`], the rules that only hold
//! across a whole document. The two are sequential because no rule can
//! run on a document that did not parse, and they are in the same
//! function because a caller that could get one without the other would
//! eventually be written.
//!
//! [`Document::EXAMPLE`] is the same shape as prose a writer can copy.
//! It is what every door publishes: the block a node's session
//! receives, the `document_shape` tool, `yunta schema`, and the shape a
//! run hands back when a read fails. One text with four consumers,
//! rather than four texts that drift apart at the first schema change.

use serde::de::DeserializeOwned;

use crate::diagnostic::{Diagnostic, DocumentRef, Malformation, Problem, Report, Rule, Subject};
use crate::yaml::Value;
use crate::{ArtifactKind, FindingsFile, Ledger, QuestionsFile};

mod walk;

pub use walk::{Hint, Walk};

/// A document whose shape the system publishes, whose failures it
/// explains, and whose rules it enforces.
///
/// Sealed: the three kinds are the schema, and a fourth is a schema
/// change rather than an extension point. Sealing is what lets the trait
/// name [`Walk`] and [`Value`] in its signatures without those becoming
/// a contract owed to implementors outside this crate.
pub trait Document: DeserializeOwned + sealed::Sealed {
    /// How every door names this document.
    const KIND: ArtifactKind;

    /// A complete, valid document of this kind, annotated field by
    /// field. Every door hands this to whoever has to write one; the
    /// test that reads it back through [`read`] is what keeps it true.
    const EXAMPLE: &'static str;

    /// Every problem a document that did not deserialize has, in
    /// document order. Called only after serde has already refused, so
    /// it never has to build a value — only to explain one.
    fn diagnose(value: &Value, walk: &mut Walk);

    /// Every rule that only holds across the whole document: an id used
    /// twice, a dependency on a task nobody declared, two independent
    /// tasks reaching for the same files. Total and pure — the document
    /// is all it needs.
    fn check(&self) -> Vec<Diagnostic>;

    /// What those rules demand, stated for whoever has to satisfy them.
    ///
    /// The same list [`check`](Document::check) enforces, read the other
    /// way round: published with the shape, before anything is written.
    /// A writer who never heard a rule pays a whole repair attempt for
    /// something the system already knew.
    const RULES: &'static [Rule];
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for crate::Ledger {}
    impl Sealed for crate::FindingsFile {}
    impl Sealed for crate::QuestionsFile {}
}

/// Reads `bytes` into `T`, or reports every problem the document has.
///
/// The only way to obtain an interpreted document, so no caller can get
/// one that skipped its rules. `path` is where a reader opens the file;
/// the kind comes from `T` itself, which is what makes a report about a
/// ledger unable to publish the shape of a questions file.
pub fn read<T: Document>(bytes: &[u8], path: impl Into<String>) -> Result<T, Report> {
    let document = DocumentRef::new(T::KIND, path);
    let one = |problem: Problem| {
        Report::new(
            document.clone(),
            vec![Diagnostic::new(Subject::Document, problem)],
        )
    };

    let Ok(text) = std::str::from_utf8(bytes) else {
        return Err(one(Problem::not_yaml(None, "the bytes are not UTF-8")));
    };

    let refusal = match crate::yaml::parse::<T>(text) {
        Ok(parsed) => {
            let broken = parsed.check();
            return if broken.is_empty() {
                Ok(parsed)
            } else {
                Err(Report::new(document, broken))
            };
        }
        Err(error) => error.to_string(),
    };

    // Whether the bytes are YAML at all decides which explanation is
    // honest: a document that never parsed has no entries to blame.
    let Ok(value) = crate::yaml::parse::<Value>(text) else {
        return Err(one(Problem::not_yaml(looks_like(text), refusal)));
    };

    let mut walk = Walk::new();
    T::diagnose(&value, &mut walk);
    let mut diagnostics = walk.into_diagnostics();
    if diagnostics.is_empty() {
        // The walk found nothing serde objected to. That is a gap in
        // this module, and it is reported as one rather than swallowed:
        // a read that fails always names at least one problem.
        diagnostics.push(Diagnostic::new(
            Subject::Document,
            Problem::unreadable(refusal),
        ));
    }
    Err(Report::new(document, diagnostics))
}

/// Everything a writer has to know to produce a document of this kind:
/// the shape, and the rules the shape cannot show.
///
/// The single place a kind turns into text, so no door can hand out a
/// different contract. The two halves answer different questions — the
/// example says what a good document looks like, the rules say what
/// makes one fail — and a writer needs both before writing, not after.
pub fn contract(kind: ArtifactKind) -> String {
    match kind {
        ArtifactKind::TaskLedger => rendered::<Ledger>(),
        ArtifactKind::Findings => rendered::<FindingsFile>(),
        ArtifactKind::Questions => rendered::<QuestionsFile>(),
    }
}

/// The rules a kind is held to, for a caller that wants them as data.
pub fn rules(kind: ArtifactKind) -> &'static [Rule] {
    match kind {
        ArtifactKind::TaskLedger => Ledger::RULES,
        ArtifactKind::Findings => FindingsFile::RULES,
        ArtifactKind::Questions => QuestionsFile::RULES,
    }
}

fn rendered<T: Document>() -> String {
    let mut text = T::EXAMPLE.trim_end().to_string();
    if T::RULES.is_empty() {
        text.push('\n');
        return text;
    }
    text.push_str("\n\n# The engine also refuses the document, and fails the node, unless:\n");
    for rule in T::RULES {
        text.push_str(&format!("#   - {}\n", crate::text::one_line(rule.demand)));
    }
    text
}

/// A malformation a writer recognizes, so the diagnostic can name the
/// cause instead of the character the scanner tripped on.
fn looks_like(text: &str) -> Option<Malformation> {
    let trimmed = text.trim_start();
    if trimmed.starts_with("```") {
        return Some(Malformation::MarkdownFence);
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Some(Malformation::JsonDocument);
    }
    // A first line with neither a key nor a list item is prose: an
    // agent explaining the file it is about to write.
    let first = trimmed.lines().find(|line| !line.trim().is_empty())?;
    let looks_structural =
        first.contains(':') || first.trim_start().starts_with('-') || first.starts_with("---");
    (!looks_structural).then_some(Malformation::LeadingProse)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// Every key a walk accepts is a key the type declares, and every
    /// key a walk demands is a key the type requires.
    ///
    /// The key lists are restated from the types rather than derived
    /// from them, because deriving them at runtime would put schema
    /// generation — and all of `schemars` — in the shipped binary. This
    /// is what makes the restatement safe: a field added to a type and
    /// not to its list fails here, instead of turning into a walk that
    /// reports a valid key as unknown.
    fn keys_match(schema: schemars::Schema, name: &str, required: &[&str], optional: &[&str]) {
        let json = serde_json::to_value(schema).expect("a schema renders");
        let definition = &json["$defs"][name];
        let declared: BTreeSet<&str> = definition["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("`{name}` declares properties: {definition}"))
            .keys()
            .map(String::as_str)
            .collect();
        let accepted: BTreeSet<&str> = required.iter().chain(optional).copied().collect();
        assert_eq!(declared, accepted, "`{name}`: the keys it declares");

        let demanded: BTreeSet<&str> = definition["required"]
            .as_array()
            .map(|items| items.iter().filter_map(|item| item.as_str()).collect())
            .unwrap_or_default();
        assert_eq!(
            demanded,
            required.iter().copied().collect::<BTreeSet<_>>(),
            "`{name}`: the keys it requires"
        );
    }

    // --- the published example shows every key, and every key it shows
    // --- is one the walk can explain
    //
    // These two are one chain. The first makes the example exhaustive over
    // the type; the second makes the walk exhaustive over the example. Held
    // together they say the thing that matters: every key a writer may write
    // was shown to them, with a value of the right type, and getting it
    // wrong names the key rather than falling back on the deserializer.

    /// Every key, at any depth, that the document writes.
    fn keys_in(value: &crate::yaml::Value, into: &mut BTreeSet<String>) {
        match value {
            crate::yaml::Value::Mapping(map) => {
                for (key, child) in map {
                    if let Some(name) = key.as_str() {
                        into.insert(name.to_string());
                    }
                    keys_in(child, into);
                }
            }
            crate::yaml::Value::Sequence(items) => {
                for item in items {
                    keys_in(item, into);
                }
            }
            _ => {}
        }
    }

    fn shown<T: Document>() -> BTreeSet<String> {
        let value: crate::yaml::Value =
            crate::yaml::parse(T::EXAMPLE).expect("the published example is YAML");
        let mut keys = BTreeSet::new();
        keys_in(&value, &mut keys);
        keys
    }

    /// A key the type accepts and the example never writes leaves a writer
    /// inferring its type from prose — which is how a boolean gets quoted.
    fn example_shows_every_key<T: Document>(levels: &[&[&str]]) {
        let shown = shown::<T>();
        let accepted: BTreeSet<&str> = levels.iter().flat_map(|l| l.iter().copied()).collect();
        let missing: Vec<&&str> = accepted
            .iter()
            .filter(|key| !shown.contains(**key))
            .collect();
        assert!(
            missing.is_empty(),
            "{}'s published example never writes {missing:?}",
            T::KIND
        );
    }

    /// Replaces the first value written under `key`, at any depth.
    fn with_wrong_value(
        value: &crate::yaml::Value,
        key: &str,
        wrong: &crate::yaml::Value,
    ) -> (crate::yaml::Value, bool) {
        match value {
            crate::yaml::Value::Mapping(map) => {
                let mut out = crate::yaml::Mapping::new();
                let mut done = false;
                for (name, child) in map {
                    if !done && name.as_str() == Some(key) {
                        out.insert(name.clone(), wrong.clone());
                        done = true;
                        continue;
                    }
                    let (child, hit) = if done {
                        (child.clone(), false)
                    } else {
                        with_wrong_value(child, key, wrong)
                    };
                    done = done || hit;
                    out.insert(name.clone(), child);
                }
                (crate::yaml::Value::Mapping(out), done)
            }
            crate::yaml::Value::Sequence(items) => {
                let mut out = Vec::with_capacity(items.len());
                let mut done = false;
                for item in items {
                    let (item, hit) = if done {
                        (item.clone(), false)
                    } else {
                        with_wrong_value(item, key, wrong)
                    };
                    done = done || hit;
                    out.push(item);
                }
                (crate::yaml::Value::Sequence(out), done)
            }
            other => (other.clone(), false),
        }
    }

    /// Every key the example writes, given a value of the wrong type, is
    /// named by a diagnostic of its own.
    ///
    /// The probes are `true` and an empty mapping: between them they are the
    /// wrong type for every shape a document key can hold, so no per-key
    /// table has to be kept here. A key with no walk check falls through to
    /// `Problem::Unreadable`, which carries the deserializer's own words —
    /// the one thing this whole frontier exists to keep from a reader.
    fn the_walk_explains_every_key<T: Document>() {
        let example: crate::yaml::Value =
            crate::yaml::parse(T::EXAMPLE).expect("the published example is YAML");
        for key in shown::<T>() {
            let explained = [
                crate::yaml::Value::Bool(true),
                crate::yaml::Value::Mapping(crate::yaml::Mapping::new()),
            ]
            .iter()
            .any(|wrong| {
                let (probe, _) = with_wrong_value(&example, &key, wrong);
                let text = crate::yaml::to_string(&probe).expect("a value renders");
                match read::<T>(text.as_bytes(), "probe.yaml") {
                    Ok(_) => false,
                    Err(report) => report.diagnostics.iter().any(|d| d.code() != "unreadable"),
                }
            });
            assert!(
                explained,
                "{}: a wrong value under `{key}` produces no diagnostic naming it — \
                 the walk has no check for that key",
                T::KIND
            );
        }
    }

    #[test]
    fn every_published_example_writes_every_key_its_type_accepts() {
        example_shows_every_key::<crate::Ledger>(&[
            crate::ledger::shape::TASK_REQUIRED,
            crate::ledger::shape::TASK_OPTIONAL,
            crate::ledger::shape::CRITERION_REQUIRED,
            crate::ledger::shape::CRITERION_OPTIONAL,
        ]);
        example_shows_every_key::<crate::FindingsFile>(&[
            crate::findings::shape::FINDING_REQUIRED,
            crate::findings::shape::FINDING_OPTIONAL,
            crate::findings::shape::PROPOSED_CRITERION_REQUIRED,
        ]);
        example_shows_every_key::<crate::QuestionsFile>(&[
            crate::questions::shape::QUESTION_REQUIRED,
            crate::questions::shape::QUESTION_OPTIONAL,
        ]);
    }

    #[test]
    fn every_key_a_writer_may_write_has_a_diagnostic_of_its_own() {
        the_walk_explains_every_key::<crate::Ledger>();
        the_walk_explains_every_key::<crate::FindingsFile>();
        the_walk_explains_every_key::<crate::QuestionsFile>();
    }

    #[test]
    fn a_task_walk_knows_exactly_the_keys_a_task_declares() {
        keys_match(
            crate::schema::ledger(),
            "Task",
            crate::ledger::shape::TASK_REQUIRED,
            crate::ledger::shape::TASK_OPTIONAL,
        );
    }

    #[test]
    fn a_criterion_walk_knows_exactly_the_keys_a_criterion_declares() {
        keys_match(
            crate::schema::ledger(),
            "Criterion",
            crate::ledger::shape::CRITERION_REQUIRED,
            crate::ledger::shape::CRITERION_OPTIONAL,
        );
    }

    #[test]
    fn a_finding_walk_knows_exactly_the_keys_a_finding_declares() {
        keys_match(
            crate::schema::findings(),
            "FindingEntry",
            crate::findings::shape::FINDING_REQUIRED,
            crate::findings::shape::FINDING_OPTIONAL,
        );
    }

    #[test]
    fn a_proposed_criterion_walk_knows_exactly_the_keys_it_declares() {
        keys_match(
            crate::schema::findings(),
            "ProposedCriterionEntry",
            crate::findings::shape::PROPOSED_CRITERION_REQUIRED,
            &[],
        );
    }

    #[test]
    fn a_question_walk_knows_exactly_the_keys_a_question_declares() {
        keys_match(
            crate::schema::questions(),
            "Question",
            crate::questions::shape::QUESTION_REQUIRED,
            crate::questions::shape::QUESTION_OPTIONAL,
        );
    }
}
