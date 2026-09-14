//! One node of the graph: what it resolved to run on, what it started,
//! what it produced or failed at, what it was re-routed to, and every
//! mechanical check the engine ran around it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::RunnerCandidate;
use crate::events::session::payloads::TokenUsage;
use crate::events::Failure;
use crate::hash::ContentHash;
use crate::ids::{NodeId, RunnerName, TaskId};

/// A tasks document criterion, frozen into `task_registered` — the same shape
/// the tasks parser produces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Criterion {
    pub cmd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<CriterionType>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CriterionType {
    Guard,
}

/// One criterion's outcome inside `criteria_checked.results`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CriterionResult {
    pub cmd: String,
    pub exit_code: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<CriterionType>,
    pub reused: bool,
    /// Wall-clock milliseconds this execution took — the datum
    /// the learned criterion ordering feeds on. `None` for a `reused: true`
    /// result (nothing ran) and for events logged before this field
    /// existed (additive, tolerant reader).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Pre,
    Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HookPhase {
    Before,
    After,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscardedCandidate {
    pub candidate: RunnerCandidate,
    pub reason: String,
}

/// One resolved `ContextSource` reference in `context_assembled.sources`.
/// `kind` stays a plain string — the engine fixes the *set* of
/// builtin source kinds (`files | command | artifact | run-events |
/// tasks | knowledge | node-output`, `mcp` among them) but never closes
/// it into an enum, since a pack can add its own sources without
/// this type needing to change. `content_hash` is what makes every
/// resolution's event carry a verifiable hash — the hash of exactly
/// the bytes the run stored under `objects/<content_hash>` for this
/// source, so replay can name precisely what a session saw without
/// re-running anything.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextSourceRef {
    pub source_id: String,
    pub kind: String,
    pub content_hash: ContentHash,
}

/// `runner` names the `runners:` entry the node resolved through. The
/// reader also accepts `role`, the field's former name, so a log written
/// under it still replays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunnerResolvedPayload {
    #[serde(alias = "role")]
    pub runner: RunnerName,
    pub chosen: RunnerCandidate,
    pub discarded: Vec<DiscardedCandidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BaselineCapturedPayload {
    pub command: String,
    pub results: BaselineResults,
    pub hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BaselineResults {
    pub exit_code: i32,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeStartedPayload {
    pub attempt: u32,
}

impl NodeStartedPayload {
    /// A node beginning its `n`th attempt, counted from one.
    ///
    /// The number is the whole payload, and every kind of node that
    /// starts — a session, a gate, a round of questions — counts it the
    /// same way, so no surface has to know which sort of node it is
    /// reading to know which try it is on.
    pub fn attempt(n: u32) -> Self {
        NodeStartedPayload { attempt: n }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextAssembledPayload {
    /// `Some` when this assembly built one *task's* brief inside a loop
    /// node — the same task-vs-node convention
    /// `ScopeCheckedPayload.task_id` already follows. `None` for a
    /// node-level assembly (a `prompt` node's own context). Additive:
    /// events logged before this field existed parse with `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<crate::ids::TaskId>,
    pub sources: Vec<ContextSourceRef>,
    /// Keys are `"stable" | "run-stable" | "volatile"` (the fixed
    /// stability classes) — kept as plain strings rather than an enum key
    /// to sidestep serde's map-key-as-enum ceremony for no real benefit.
    pub segment_hashes: BTreeMap<String, ContentHash>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CriteriaCheckedPayload {
    pub task_id: TaskId,
    pub phase: Phase,
    pub results: Vec<CriterionResult>,
}

/// `task_id` is present for a task's scope check within a loop node;
/// absent for a node-level scope check — either way the envelope's own
/// `node_id` already names the node, so it is not repeated here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ScopeCheckedPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    pub diff: Vec<PathBuf>,
    pub violations: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeFinishedPayload {
    pub outcome: String,
    pub tokens_used: TokenUsage,
}

impl NodeFinishedPayload {
    /// A node closed, with what it did and what it spent doing it.
    ///
    /// The spend is the attempt's own: a node whose accounting already
    /// closed elsewhere — one that paid for its session when it asked
    /// its questions — passes nothing, so no surface counts the same
    /// tokens twice.
    pub fn new(outcome: impl Into<String>, tokens: TokenUsage) -> Self {
        NodeFinishedPayload {
            outcome: outcome.into(),
            tokens_used: tokens,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeFailedPayload {
    /// What went wrong, as data. The prose a reader sees is produced
    /// from this on read, so no rendering can disagree with the facts
    /// behind it — and a receipt counts what failed without parsing a
    /// sentence.
    #[serde(flatten)]
    pub failure: Failure,
    pub tokens_used: TokenUsage,
    /// Whether this node will be attempted again. Set by whoever owns
    /// the budget, so a failure that nothing will retry is never
    /// recorded as retryable.
    pub retryable: bool,
}

impl NodeFailedPayload {
    pub fn new(failure: Failure, retryable: bool, tokens_used: TokenUsage) -> Self {
        NodeFailedPayload {
            failure,
            tokens_used,
            retryable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HookExecutedPayload {
    pub phase: HookPhase,
    pub command: String,
    pub exit_code: i32,
}

/// `from_node` is the envelope's own `node_id` (the node that failed) —
/// only the re-route's destination is extra information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeReroutedPayload {
    pub to_node: NodeId,
    pub cause: String,
    /// The retry count against the cap — present only for an
    /// `on_failure` reroute, absent for a gate's routing choice, which
    /// is not a retry and has no cap to count against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_reroutes: Option<u32>,
    /// Which mechanism rerouted. Old logs, written before the
    /// distinction existed, carry no field and read as `OnFailure`.
    #[serde(default)]
    pub origin: RerouteOrigin,
}

impl NodeReroutedPayload {
    /// Control handed to another node, and why.
    ///
    /// The retry count belongs to an `on_failure` re-route and to
    /// nothing else: a gate's routing choice is a decision, not a retry,
    /// and has no cap to count against. Taking the origin and the count
    /// together is what keeps a gate's route from being read as a node
    /// on its last try.
    pub fn new(
        to: NodeId,
        cause: impl Into<String>,
        origin: RerouteOrigin,
        attempt: Option<u32>,
        max_reroutes: Option<u32>,
    ) -> Self {
        let retrying = matches!(origin, RerouteOrigin::OnFailure);
        NodeReroutedPayload {
            to_node: to,
            cause: cause.into(),
            attempt: attempt.filter(|_| retrying),
            max_reroutes: max_reroutes.filter(|_| retrying),
            origin,
        }
    }
}

/// What caused a `node_rerouted`. The two mechanisms differ in kind: an
/// `on_failure` reroute is a bounded retry (it carries counters); a
/// gate's `on:` choice is a routing decision (it carries none).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RerouteOrigin {
    #[default]
    OnFailure,
    GateChoice,
}
