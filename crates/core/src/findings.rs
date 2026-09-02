//! The `kind: findings` artifact — what a reviewing session writes and
//! the engine reads at node close. An agent wrote it, so every type
//! here refuses a key it does not know; the log records each entry as
//! [`events::Finding`], which reads what a later writer adds.

use serde::{Deserialize, Serialize};

use crate::events::{self, FindingSeverity};
use crate::ids::FindingId;

/// The artifact's document — sole top-level key `findings:`, mirroring
/// a ledger's `tasks:`-only shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// A criterion the author proposes to verify the finding's fix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
