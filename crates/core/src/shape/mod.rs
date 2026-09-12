//! Reading a document an agent wrote, and publishing the shape it
//! should have written.
//!
//! [`read`] and [`accept`] parse with serde, so the schema is declared
//! exactly once — in the types — and never restated here. A refusal
//! names the path of the value it refused, which is the key and what was
//! expected of it; [`render`] is the way back, and the bytes the engine
//! writes for a document of this kind.
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

use crate::diagnostic::{Diagnostic, DocumentRef, Problem, Report, Rule, Subject};
use crate::{ArtifactKind, FindingsFile, Ledger, QuestionsFile};

/// A document whose shape the system publishes, whose failures it
/// explains, and whose rules it enforces.
///
/// Sealed: the three kinds are the schema, and a fourth is a schema
/// change rather than an extension point. Sealing is what lets the trait
/// name [`Diagnostic`] and [`Rule`] in its signatures without those
/// becoming a contract owed to implementors outside this crate.
pub trait Document: DeserializeOwned + serde::Serialize + sealed::Sealed {
    /// How every door names this document.
    const KIND: ArtifactKind;

    /// A complete, valid document of this kind, annotated field by
    /// field. Every door hands this to whoever has to write one; the
    /// test that reads it back through [`read`] is what keeps it true.
    const EXAMPLE: &'static str;

    /// Every rule that only holds across the whole document: an id used
    /// twice, a dependency on a task nobody declared, two independent
    /// tasks reaching for the same files. Total and pure — the document
    /// is all it needs.
    fn check(&self) -> Vec<Diagnostic>;

    /// What those rules demand, stated for whoever has to satisfy them.
    ///
    /// The same list [`check`](Document::check) enforces, read the other
    /// way round: published with the shape, before anything is written.
    /// A writer who never heard a rule pays a whole attempt for
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
        return Err(one(Problem::parse("", "the bytes are not UTF-8")));
    };

    match crate::yaml::parse::<T>(text) {
        Ok(parsed) => {
            let broken = parsed.check();
            if broken.is_empty() {
                Ok(parsed)
            } else {
                Err(Report::new(document, broken))
            }
        }
        Err(crate::yaml::YamlError::Parse { path, message }) => {
            Err(one(Problem::parse(path, message)))
        }
        Err(other) => Err(one(Problem::parse("", other.to_string()))),
    }
}

/// Reads a document a session submitted, or reports every problem it
/// has.
///
/// The counterpart of [`read`] for a document that never was a file: it
/// arrives as a structured value, so the deserializer is the only thing
/// that differs — the type is the same, the rules it then has to satisfy
/// are the same, and so is the report a caller gets back. `path` is what
/// the report names the document by; a submission has no file yet, so
/// callers pass the name the file will have, or the tool that carried
/// it.
pub fn accept<T: Document>(
    document: serde_json::Value,
    path: impl Into<String>,
) -> Result<T, Report> {
    let document_ref = DocumentRef::new(T::KIND, path);
    match serde_path_to_error::deserialize::<_, T>(document) {
        Ok(parsed) => {
            let broken = parsed.check();
            if broken.is_empty() {
                Ok(parsed)
            } else {
                Err(Report::new(document_ref, broken))
            }
        }
        Err(error) => Err(Report::new(
            document_ref,
            vec![Diagnostic::new(
                Subject::Document,
                Problem::parse(error.path().to_string(), error.into_inner().to_string()),
            )],
        )),
    }
}

/// The canonical YAML of a document the engine holds: the bytes it
/// writes for a file of this kind.
///
/// One document renders to one text, so two sessions that mean the same
/// thing produce the same file and the same hash — which is what lets a
/// memo recognize the work and a replay reproduce it.
pub fn render<T: Document>(document: &T) -> Result<String, crate::yaml::YamlError> {
    crate::yaml::to_string(document)
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// Every key a type declares is a key its published example writes.
    ///
    /// The example is what a writer copies, so a key it never shows is a
    /// key nobody knows about. The list comes from the type's own
    /// schema, which is what makes the check exhaustive rather than a
    /// second list to keep in step.
    fn example_writes_every_key<T: Document>(schema: schemars::Schema, name: &str) {
        let json = serde_json::to_value(schema).expect("a schema renders");
        let declared: BTreeSet<&str> = json["$defs"][name]["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("`{name}` declares properties"))
            .keys()
            .map(String::as_str)
            .collect();
        let example = T::EXAMPLE;
        for key in declared {
            assert!(
                example.contains(&format!("{key}:")),
                "`{name}`'s example writes `{key}`"
            );
        }
    }

    #[test]
    fn every_published_example_writes_every_key_its_type_accepts() {
        example_writes_every_key::<Ledger>(schemars::schema_for!(Ledger), "Task");
        example_writes_every_key::<Ledger>(schemars::schema_for!(Ledger), "Criterion");
        example_writes_every_key::<FindingsFile>(
            schemars::schema_for!(FindingsFile),
            "FindingEntry",
        );
        example_writes_every_key::<FindingsFile>(
            schemars::schema_for!(FindingsFile),
            "ProposedCriterionEntry",
        );
        example_writes_every_key::<QuestionsFile>(schemars::schema_for!(QuestionsFile), "Question");
    }

    /// Every published example is a document its own kind accepts.
    #[test]
    fn every_published_example_reads_back_through_its_own_door() {
        read::<Ledger>(Ledger::EXAMPLE.as_bytes(), "example").expect("the ledger example");
        read::<FindingsFile>(FindingsFile::EXAMPLE.as_bytes(), "example")
            .expect("the findings example");
        read::<QuestionsFile>(QuestionsFile::EXAMPLE.as_bytes(), "example")
            .expect("the questions example");
    }

    /// Every rule a kind is held to is a rule its contract states, so a
    /// writer never pays an attempt for something the system knew.
    #[test]
    fn every_contract_states_every_rule_its_kind_enforces() {
        for kind in ArtifactKind::ALL {
            let contract = contract(kind);
            for rule in rules(kind) {
                assert!(
                    contract.contains(&crate::text::one_line(rule.demand)),
                    "`{kind}`'s contract states `{}`",
                    rule.code
                );
            }
        }
    }

    /// Every rule code belongs to some kind's published rules: a rule
    /// the engine can report is a rule a writer was told about.
    #[test]
    fn every_rule_code_belongs_to_a_published_contract() {
        let published: BTreeSet<crate::diagnostic::RuleCode> = ArtifactKind::ALL
            .into_iter()
            .flat_map(|kind| rules(kind).iter().map(|rule| rule.code))
            .collect();
        for code in crate::diagnostic::RuleCode::ALL {
            assert!(
                published.contains(code),
                "`{code}` belongs to some kind's published rules"
            );
        }
    }
}
