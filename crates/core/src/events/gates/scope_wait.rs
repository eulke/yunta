//! Why a loop waits on a scope request a person answers, typed: the reason
//! the grant did not come by itself, and the criterion the session proposed
//! with what the engine found when it ran it. A surface writes the words from
//! these values; the escalation carries no sentence of its own for them.

use serde::{Deserialize, Serialize};

use super::escalation::Escalation;
use super::payloads::GateWaitingPayload;
use crate::expansion::{Precheck, Waiting};

/// What a loop waits on in one scope request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScopeWait {
    /// Why the request is not decided without a person.
    pub because: Waiting,
    /// The criterion the session proposed for the request, if it proposed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed: Option<ProposedCheck>,
}

/// A criterion a session proposed, and what running it found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProposedCheck {
    /// The command as the session wrote it.
    pub cmd: String,
    /// What the engine's own run of the command found.
    pub precheck: Precheck,
}

impl Escalation {
    /// The same escalation, saying why the loop waits on it.
    pub fn with_scope(mut self, scope: ScopeWait) -> Self {
        self.0.scope = Some(scope);
        self
    }
}

impl GateWaitingPayload {
    /// Why the loop waits on this request, when a scope rule holds it for a person.
    pub fn scope(&self) -> Option<&ScopeWait> {
        self.scope.as_ref()
    }
}
