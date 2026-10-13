//! A task or a node asking to write outside the scope it declared, and
//! the answer it got.

use serde::{Deserialize, Serialize};

use crate::expansion::Denial;
use crate::glob::ScopeGlob;
use crate::hash::CommitSha;
use crate::ids::{Responder, TaskId};
use crate::policy::ScopeExpansionMode;

/// `decided_by`: `rule | person | evidence`, with who the person was, or
/// which criterion's output was the evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Decider {
    Rule,
    Person {
        id: Responder,
    },
    /// What a red criterion of the task printed located the paths: a fact
    /// the engine verified, not a choice anyone made.
    Evidence {
        criterion: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProposedCriterion {
    pub cmd: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScopeExpansionRequestedPayload {
    /// The task that asked. Absent when the session that asked works a
    /// node of its own rather than a task: the request is then that
    /// node's, the one the event is written under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
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
    /// The task the grant widens. Absent for a grant to a node's own
    /// scope — the node the event is written under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    pub decided_by: Decider,
    /// The mode in force when a loop granted it. A node's own grant has
    /// none, and reads back without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ScopeExpansionMode>,
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
    /// Absent when the denied request was the node's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    pub decided_by: Decider,
    /// The mode in force when a loop denied it. A node's own denial has
    /// none, and reads back without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ScopeExpansionMode>,
    pub count_this_run: u32,
    /// Why it was denied, when the decider said. A denial logged as text
    /// before reasons were typed is read by `why()` instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<Denial>,
    /// The text a denial logged before reasons were typed carried. Read,
    /// never written: `why()` reads it as `Denial::Legacy`.
    #[serde(default, rename = "denial_reason", skip_serializing)]
    #[schemars(skip)]
    pub legacy_text: Option<String>,
}

impl ScopeExpansionDeniedPayload {
    /// The reason the denial gave: its typed `why`, or the text of an old
    /// log read as `Denial::Legacy`.
    pub fn why(&self) -> Option<Denial> {
        self.why.clone().or_else(|| {
            self.legacy_text
                .as_ref()
                .map(|text| Denial::Legacy { text: text.clone() })
        })
    }
}

/// What a task may write beyond its declared scope because a shape it owns
/// is named there — derived by the engine from the workflow's tasks and the
/// run's tree, never asked for and never decided.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScopeDerivedPayload {
    pub task_id: TaskId,
    /// Each file, exactly, outside the task's declared scope that names a
    /// shape it owns. Replaces whatever an earlier derivation stated.
    pub paths: Vec<ScopeGlob>,
    /// The shapes those files name.
    pub shapes: Vec<String>,
    /// The shapes the task owns that so many files name that following
    /// them would reach half the tree: they reach nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub common: Vec<String>,
    /// The commit the files were read at.
    pub at: CommitSha,
}
