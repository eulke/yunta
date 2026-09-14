//! A run this run started, and the iterations of the loop that started it.

use serde::{Deserialize, Serialize};

use crate::events::run::payloads::TerminalState;
use crate::events::session::payloads::TokenUsage;
use crate::hash::ContentHash;
use crate::ids::RunId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LoopIterationPayload {
    pub iteration: u32,
    pub until_result: bool,
}

/// The parent's `kind: workflow` node is the envelope's own `node_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChildRunCreatedPayload {
    pub child_run_id: RunId,
    pub child_workflow_hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChildRunFinishedPayload {
    pub child_run_id: RunId,
    pub child_workflow_hash: ContentHash,
    pub terminal_state: TerminalState,
    /// The child run's whole derived spend at its close — a child
    /// run's usage always aggregates up into its parent, so replay adds
    /// it to the parent's own total. That means a promotion *chain*'s
    /// every member counts exactly once, resumes included, and the
    /// parent node's own `node_finished` carries no child tokens
    /// (they'd double-count). Additive: events logged before this field
    /// existed parse as zero.
    #[serde(default)]
    pub tokens: TokenUsage,
}
