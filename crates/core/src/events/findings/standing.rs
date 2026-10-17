//! The findings standing in a run, as one view: each with the node that
//! reported it — none for one the engine reported about the run itself
//! — and how other nodes answered it.
//!
//! What a session reads through the run's tools and what a gate shows a
//! person about the run's findings are this one view, so the two never
//! disagree about what stands — or what settled it.

use serde::{Deserialize, Serialize};

use super::ledger::{AnswerGiven, Proof, Settled};
use crate::events::Finding;
use crate::ids::NodeId;

/// Every finding standing in a run, in the order each was first posted.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RunFindings {
    pub findings: Vec<StandingFinding>,
}

/// One finding as it stands: who reported it, what it is, and what other
/// nodes answered about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StandingFinding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
    #[serde(flatten)]
    pub finding: Finding,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<AnswerGiven>,
    /// The last time the criterion it proposes ran after an answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof: Option<Proof>,
    /// What settled it, when something did: it still stands, and no
    /// longer counts against the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settled: Option<Settled>,
}
