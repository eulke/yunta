//! An agent session: the CLI it opened on, what it said while it ran,
//! and a capability the adapter did not have.

use serde::{Deserialize, Serialize};

use crate::ids::{AdapterId, AgentName, ModelName, SessionId};
use crate::{Capabilities, Capability};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached: Option<u64>,
}

impl TokenUsage {
    /// Input plus output — the single figure token cost is measured by,
    /// derived in one place so no two call sites can add it up differently.
    /// `cached` is a subset of `input`, already counted, never added on top.
    pub fn total(&self) -> u64 {
        self.input + self.output
    }
}

/// Field by field; `cached` stays unknown only while nobody reported it.
impl std::ops::Add for TokenUsage {
    type Output = TokenUsage;

    fn add(self, other: TokenUsage) -> TokenUsage {
        TokenUsage {
            input: self.input + other.input,
            output: self.output + other.output,
            cached: match (self.cached, other.cached) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
            },
        }
    }
}

impl std::ops::AddAssign for TokenUsage {
    fn add_assign(&mut self, other: TokenUsage) {
        *self = *self + other;
    }
}

impl std::iter::Sum for TokenUsage {
    fn sum<I: Iterator<Item = TokenUsage>>(iter: I) -> TokenUsage {
        iter.fold(TokenUsage::default(), |total, usage| total + usage)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentSessionOpenedPayload {
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentName>,
    /// The model the CLI reported for the session; absent when it
    /// reported none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelName>,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageType {
    ToolUse,
    Usage,
    Note,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentMessagePayload {
    pub message_type: AgentMessageType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CapabilityDegradedPayload {
    /// The capability the engine consulted and the adapter does not declare.
    pub capability: Capability,
    pub adapter: AdapterId,
    policy_applied: String,
}

impl CapabilityDegradedPayload {
    /// A capability the adapter does not have, and what the engine did
    /// instead.
    ///
    /// Both halves are the point: a degradation that names only what was
    /// missing leaves a reader guessing whether anything still holds,
    /// and one that names only the fallback hides that something was
    /// asked for and refused. The engine writes them together or not at
    /// all.
    pub fn new(
        capability: Capability,
        adapter: AdapterId,
        policy_applied: impl Into<String>,
    ) -> Self {
        CapabilityDegradedPayload {
            capability,
            adapter,
            policy_applied: policy_applied.into(),
        }
    }

    /// What the engine did instead.
    pub fn policy_applied(&self) -> &str {
        &self.policy_applied
    }
}
