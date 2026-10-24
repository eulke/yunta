//! A task or a node asking to write outside the scope it declared, and
//! the answer it got.

use serde::{Deserialize, Serialize};

use crate::events::LoggedWords;
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

/// A command a session proposes with a scope request or a finding: one that
/// fails when proposed and passes once the work it asks for is done.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProposedCriterion {
    pub cmd: String,
}

/// What a session asked to write beyond its scope, as it asked it: never a
/// request the engine rewrote. Written when the request is decided — at
/// the call, for a refusal no work could change, and otherwise at the
/// close of the attempt or the node that received it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScopeExpansionRequestedPayload {
    /// The task that asked. Absent when the session that asked works a
    /// node of its own rather than a task: the request is then that
    /// node's, the one the event is written under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// The globs it asked to be allowed.
    pub paths: Vec<ScopeGlob>,
    /// Its reason, in its own words.
    pub reason: String,
    /// A criterion it proposed with the request, when it proposed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_criterion: Option<ProposedCriterion>,
    /// What the proposed criterion did when the decision ran it before any
    /// work; absent when the decision did not run it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_criterion_precheck: Option<ProposedCriterionPrecheck>,
}

/// The code a proposed criterion exited with before the work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProposedCriterionPrecheck {
    pub exit_code: i32,
}

/// A request for scope granted: who decided, and exactly what it allowed.
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
    /// The engine's grants in this run once this decision is made: what
    /// `max_per_run` caps. A person's grant is not one of them.
    pub count_this_run: u32,
    /// The exact paths this grant authorized — self-contained
    /// audit, and what a later attempt's effective scope derives from
    /// the log, instead of re-pairing the grant with the
    /// `requested` event that preceded it. `default` for logs written
    /// before the field existed (tolerant reader).
    #[serde(default)]
    pub paths: Vec<ScopeGlob>,
}

/// A request for scope refused, and why. Every refusal also posts a
/// finding that says so.
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
    /// The engine's grants in this run when the request was refused.
    pub count_this_run: u32,
    /// Why it was denied, when the decider said. A denial logged as text
    /// before reasons were typed is read by `why()` instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<Denial>,
    /// The text a denial logged before reasons were typed carried. Read from
    /// such a log and written back as read; `why()` reads it as
    /// `Denial::Legacy`.
    #[serde(
        default,
        rename = "denial_reason",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(skip)]
    pub legacy_text: Option<LoggedWords>,
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
/// run's tree, never asked for and never decided. Not a grant: it counts
/// toward no cap.
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

/// Why a request left without an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WithdrawalCause {
    /// The task closed while its request still waited for an answer.
    TaskClosed,
}

/// A request its task closed with, unanswered. Nobody decided it, so
/// nothing was granted or denied: the request leaves with the task,
/// posting no finding and counting toward no cap. A task that does not
/// close keeps its request for a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScopeExpansionWithdrawnPayload {
    pub task_id: TaskId,
    pub paths: Vec<ScopeGlob>,
    pub reason: WithdrawalCause,
}
