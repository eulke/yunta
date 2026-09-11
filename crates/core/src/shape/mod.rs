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

use crate::diagnostic::{Diagnostic, DocumentRef, Malformation, Problem, Report, Subject};
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

/// The shape published for a kind: what every door hands to whoever has
/// to write one. The single place a kind maps to its text, so no door
/// can render a different one.
pub fn published(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::TaskLedger => Ledger::EXAMPLE,
        ArtifactKind::Findings => FindingsFile::EXAMPLE,
        ArtifactKind::Questions => QuestionsFile::EXAMPLE,
    }
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
