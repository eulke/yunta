//! A findings document as the run holds it: the entries a session wrote, and
//! what the engine found, each with its cause. The engine renders one when it
//! derives a document, and every reader of a document the run already holds
//! reads one. A session never writes one: it hands over a findings file, whose
//! entries carry their words.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::rules;
use super::{FindingEntry, FindingFault, Location, ProposedCriterionEntry};
use crate::diagnostic::{Diagnostic, Named, Problem, Rule, Subject};
use crate::events::findings::{FindingCause, FindingSays};
use crate::events::{self, FindingSeverity};
use crate::{ArtifactKind, FindingId};

/// A findings document as the run holds it: every finding a session wrote,
/// with its words, and every finding the engine reported, with its cause.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HeldFindings {
    pub findings: Vec<HeldEntry>,
}

/// One entry of a held document: a session's, which is the entry it wrote,
/// or the engine's, with its cause.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum HeldEntry {
    Written(FindingEntry),
    Found(FoundEntry),
}

/// A finding the engine reported, as a document holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FoundEntry {
    /// Unique among the findings the document holds.
    pub id: FindingId,
    pub severity: FindingSeverity,
    pub location: Location,
    pub cause: FindingCause,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_criterion: Option<ProposedCriterionEntry>,
}

impl HeldFindings {
    /// The document that carries `findings` forward: a written finding is the
    /// entry its words make, and a found one keeps its cause.
    pub fn from_findings(findings: Vec<events::Finding>) -> Self {
        HeldFindings {
            findings: findings.into_iter().map(HeldEntry::from).collect(),
        }
    }
}

impl HeldEntry {
    /// The id the entry is declared under, whichever way it says itself.
    fn id(&self) -> &FindingId {
        match self {
            HeldEntry::Written(entry) => &entry.id,
            HeldEntry::Found(found) => &found.id,
        }
    }
}

impl From<events::Finding> for HeldEntry {
    fn from(finding: events::Finding) -> Self {
        let proposed_criterion: Option<ProposedCriterionEntry> =
            finding.proposed_criterion.map(Into::into);
        match finding.says {
            FindingSays::Written { title, detail } => HeldEntry::Written(FindingEntry {
                id: finding.id,
                severity: finding.severity,
                title,
                location: finding.location,
                detail,
                proposed_criterion,
            }),
            FindingSays::Found { cause } => HeldEntry::Found(FoundEntry {
                id: finding.id,
                severity: finding.severity,
                location: finding.location,
                cause,
                proposed_criterion,
            }),
        }
    }
}

impl From<HeldEntry> for events::Finding {
    fn from(entry: HeldEntry) -> Self {
        match entry {
            HeldEntry::Written(written) => written.into(),
            HeldEntry::Found(found) => events::Finding {
                id: found.id,
                severity: found.severity,
                says: FindingSays::Found { cause: found.cause },
                location: found.location,
                proposed_criterion: found.proposed_criterion.map(Into::into),
            },
        }
    }
}

/// Read the way a findings document a session wrote is read: the rules of
/// that document hold each written entry, and the id rule holds every entry.
impl crate::shape::Document for HeldFindings {
    const KIND: ArtifactKind = ArtifactKind::Findings;
    const EXAMPLE: &'static str = super::EXAMPLE;

    fn check(&self) -> Vec<Diagnostic> {
        let mut broken = Vec::new();
        let mut known_ids: HashSet<&FindingId> = HashSet::new();
        for (index, entry) in self.findings.iter().enumerate() {
            let id = entry.id();
            if !known_ids.insert(id) {
                broken.push(Diagnostic::new(
                    Subject::Finding(Named::new(id.clone(), index)),
                    Problem::broke(FindingFault::DuplicateFinding),
                ));
            }
            if let HeldEntry::Written(written) = entry {
                broken.extend(rules::check_entry(written, index));
            }
        }
        broken
    }

    const RULES: &'static [Rule] = rules::RULES;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::findings::FindingsFile;
    use crate::shape::{read, render, Document};

    /// A held document of entries a session wrote is the findings file they
    /// came from, byte for byte: what a session reads back is what the run
    /// holds.
    #[test]
    fn a_held_document_of_session_entries_renders_as_its_findings_file() {
        let file: FindingsFile = read(<FindingsFile as Document>::EXAMPLE.as_bytes(), "example")
            .expect("the findings example reads");
        let held = HeldFindings::from_findings(
            file.findings
                .iter()
                .cloned()
                .map(events::Finding::from)
                .collect(),
        );
        assert_eq!(
            render(&held).expect("a held document renders"),
            render(&file).expect("a findings file renders"),
        );
    }

    /// An id names one finding in a document, whether a session wrote the
    /// finding or the engine found it.
    #[test]
    fn an_id_is_unique_across_the_entries_a_session_wrote_and_the_engine_found() {
        let written = HeldEntry::Written(FindingEntry {
            id: "f-1".into(),
            severity: FindingSeverity::Minor,
            title: "a title".to_string(),
            location: "src/a.rs:1".into(),
            detail: "a detail".to_string(),
            proposed_criterion: None,
        });
        let found = HeldEntry::Found(FoundEntry {
            id: "f-1".into(),
            severity: FindingSeverity::Minor,
            location: "src/a.rs:1".into(),
            cause: FindingCause::CleanupSkipped,
            proposed_criterion: None,
        });
        let held = HeldFindings {
            findings: vec![written, found],
        };
        assert_eq!(
            held.check().len(),
            1,
            "the engine's `f-1` repeats the session's"
        );
    }
}
