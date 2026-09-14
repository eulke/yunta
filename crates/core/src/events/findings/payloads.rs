//! What a session reported about work that is not its own to fix: posted,
//! updated, withdrawn, or refused by the engine.

use serde::{Deserialize, Serialize};

use crate::events::scope::payloads::ProposedCriterion;
use crate::findings::Location;
use crate::ids::FindingId;

/// `severity`: `blocking | major | minor | note` — confirmed against the
/// `kind: findings` schema, not inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Blocking,
    Major,
    Minor,
    Note,
}

impl FindingSeverity {
    /// The ladder, as a document writes it. Tied to what serde derives
    /// by a test, so a diagnostic listing the ladder cannot list a
    /// different one from the parser accepting it.
    pub const NAMES: [&'static str; 4] = ["blocking", "major", "minor", "note"];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Finding {
    pub id: FindingId,
    pub severity: FindingSeverity,
    pub title: String,
    pub location: Location,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_criterion: Option<ProposedCriterion>,
}

/// The finding's author is the envelope's own `node_id` — not repeated
/// here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingPostedPayload {
    pub finding: Finding,
}

/// The node's finding, in its new state — a whole replacement, never a
/// merge: a field absent from an update is absent from the finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingUpdatedPayload {
    pub finding: Finding,
}

/// The node's finding `id` no longer stands, and why — in the words of
/// whoever took it back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingWithdrawnPayload {
    pub id: FindingId,
    pub reason: String,
}

/// Which of the three a refused call was making.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindingOperation {
    Post,
    Update,
    Withdraw,
}

/// A finding a session offered and the engine did not take, with every
/// problem named. The accepted cases are the three events above; this is
/// what a session was told instead, kept so the rate a run gets findings
/// wrong is a fact about the run rather than something only the session
/// saw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingRefusedPayload {
    pub operation: FindingOperation,
    /// The id the call named, when it named one that parses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<FindingId>,
    pub report: crate::diagnostic::Report,
}
