//! A node asking to write outside the scope it declared, and the answer
//! it got.

use serde::{Deserialize, Serialize};

use crate::glob::ScopeGlob;
use crate::ids::{Responder, TaskId};
use crate::policy::ScopeExpansionMode;

/// `decided_by`: `rule | person` plus an identifier for the latter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Decider {
    Rule,
    Person { id: Responder },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProposedCriterion {
    pub cmd: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScopeExpansionRequestedPayload {
    pub task_id: TaskId,
    pub paths: Vec<ScopeGlob>,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_criterion: Option<ProposedCriterion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_criterion_precheck: Option<ProposedCriterionPrecheck>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProposedCriterionPrecheck {
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScopeExpansionGrantedPayload {
    pub task_id: TaskId,
    pub decided_by: Decider,
    pub mode: ScopeExpansionMode,
    pub count_this_run: u32,
    /// The exact paths this grant authorized — self-contained
    /// audit, and what a later attempt's effective scope derives from
    /// the log, instead of re-pairing the grant with the
    /// `requested` event that preceded it. `default` for logs written
    /// before the field existed (tolerant reader).
    #[serde(default)]
    pub paths: Vec<ScopeGlob>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScopeExpansionDeniedPayload {
    pub task_id: TaskId,
    pub decided_by: Decider,
    pub mode: ScopeExpansionMode,
    pub count_this_run: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denial_reason: Option<String>,
}
