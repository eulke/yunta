//! The `kind: findings` artifact — what a reviewing session writes and
//! the engine reads at node close. An agent wrote it, so every type
//! here refuses a key it does not know; the log records each entry as
//! [`events::Finding`], which reads what a later writer adds.
//!
//! The shape published to whoever writes one, and the rules that hold
//! across the whole document, live alongside the types: they are one
//! schema.

use serde::{Deserialize, Serialize};

use crate::events::{self, FindingSeverity};
use crate::ids::FindingId;

/// The artifact's document — sole top-level key `findings:`, mirroring
/// a tasks document's `tasks:`-only shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FindingsFile {
    pub findings: Vec<FindingEntry>,
}

impl FindingsFile {
    /// The document that carries `findings` forward — what a run
    /// writes for its successor to inherit.
    pub fn from_findings(findings: Vec<events::Finding>) -> Self {
        FindingsFile {
            findings: findings.into_iter().map(FindingEntry::from).collect(),
        }
    }
}

/// One finding as the artifact declares it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FindingEntry {
    pub id: FindingId,
    pub severity: FindingSeverity,
    pub title: String,
    pub location: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_criterion: Option<ProposedCriterionEntry>,
}

/// Taking one finding back: which, and why.
///
/// A document like any other — strict about its keys, with a rule of its
/// own — because it reaches the engine the same way a finding does, and
/// a withdrawal nobody can explain is a finding that disappeared.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Withdrawal {
    pub id: FindingId,
    pub reason: String,
}

impl Withdrawal {
    /// What the document owes once its keys are known: a reason with
    /// something in it.
    pub fn check(&self) -> Vec<crate::diagnostic::Diagnostic> {
        rules::check_withdrawal(self)
    }
}

/// A criterion the author proposes to verify the finding's fix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposedCriterionEntry {
    pub cmd: String,
}

impl From<FindingEntry> for events::Finding {
    fn from(entry: FindingEntry) -> Self {
        events::Finding {
            id: entry.id,
            severity: entry.severity,
            title: entry.title,
            location: entry.location,
            detail: entry.detail,
            proposed_criterion: entry.proposed_criterion.map(Into::into),
        }
    }
}

impl From<events::Finding> for FindingEntry {
    fn from(finding: events::Finding) -> Self {
        FindingEntry {
            id: finding.id,
            severity: finding.severity,
            title: finding.title,
            location: finding.location,
            detail: finding.detail,
            proposed_criterion: finding.proposed_criterion.map(Into::into),
        }
    }
}

impl From<ProposedCriterionEntry> for events::ProposedCriterion {
    fn from(entry: ProposedCriterionEntry) -> Self {
        events::ProposedCriterion { cmd: entry.cmd }
    }
}

impl From<events::ProposedCriterion> for ProposedCriterionEntry {
    fn from(criterion: events::ProposedCriterion) -> Self {
        ProposedCriterionEntry { cmd: criterion.cmd }
    }
}

mod rules;

/// The shape this document publishes, as the YAML it is.
///
/// It lives as a file rather than a string literal, so an editor reads
/// it as YAML and a person reviewing a schema change sees the diff in
/// the format the change is about. `include_str!` binds it at compile
/// time, and the test that reads it back through
/// [`read`](crate::shape::read) is what stops it drifting from the
/// parser.
const EXAMPLE: &str = include_str!("shape.yaml");

impl crate::shape::Document for FindingsFile {
    const KIND: crate::ArtifactKind = crate::ArtifactKind::Findings;
    const EXAMPLE: &'static str = EXAMPLE;

    fn check(&self) -> Vec<crate::diagnostic::Diagnostic> {
        rules::check(self)
    }

    const RULES: &'static [crate::diagnostic::Rule] = rules::RULES;
}
