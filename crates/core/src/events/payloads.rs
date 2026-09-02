//! Per-kind payload structs. Field names and
//! optionality match the reference event documentation field for field;
//! anything provisional there carries the same note here.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::RunnerCandidate;
use crate::ids::{
    AdapterId, AgentName, FindingId, ModeName, ModelName, NodeId, RunId, RunnerName, Seq,
    SessionId, TaskId,
};
use crate::policy::ScopeExpansionMode;
use crate::workflow::OnInterrupt;
use crate::Capabilities;

/// A ledger criterion, frozen into `task_registered` — the same shape
/// the ledger parser produces.
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

/// Exact variant names are provisional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Ready,
    Running,
    Done,
    Blocked,
    Failed,
}

/// `decided_by`: `rule | person` plus an identifier for the latter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Decider {
    Rule,
    Person { id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProposedCriterion {
    pub cmd: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HookPhase {
    Before,
    After,
}

/// One choice in a gate's escalation: `id` is what
/// `GateResolvedPayload.chosen_option` names back, `label` is the
/// human-facing text, `tradeoff` is mandatory — any option that expands
/// scope of work must declare what it trades off, and no variant of
/// this type can omit it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GateOption {
    pub id: String,
    pub label: String,
    pub tradeoff: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Tty,
    Mcp,
    Pr,
}

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Finding {
    pub id: FindingId,
    pub severity: FindingSeverity,
    pub title: String,
    pub location: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_criterion: Option<ProposedCriterion>,
}

/// Exact variant names are provisional; a `cancel` command
/// exists, so `Cancelled` is included alongside the obvious two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TerminalState {
    Done,
    Failed,
    Cancelled,
    /// The run closed because its own gate accepted promotion
    /// to a later-declared mode — never because the work itself failed
    /// or was cancelled.
    Promoted,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached: Option<u64>,
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
pub struct DiscardedCandidate {
    pub candidate: RunnerCandidate,
    pub reason: String,
}

/// One resolved `ContextSource` reference in `context_assembled.sources`.
/// `kind` stays a plain string — the engine fixes the *set* of
/// builtin source kinds (`files | command | artifact | run-events |
/// ledger | knowledge | node-output`, `mcp` among them) but never closes
/// it into an enum, since a pack can add its own sources without
/// this type needing to change. `content_hash` is what makes every
/// resolution's event carry a verifiable hash — the hash of exactly
/// the bytes materialized under `context/<content_hash>/` for this
/// source, so replay can name precisely what a session saw without
/// re-running anything.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextSourceRef {
    pub source_id: String,
    pub kind: String,
    pub content_hash: String,
}

// --- Per-kind payloads ------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunCreatedPayload {
    pub manifest_hash: String,
    pub inputs: BTreeMap<String, serde_json::Value>,
    pub mode: ModeName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promoted_from: Option<RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yunta_schema: Option<String>,
    pub base_branch: String,
    pub base_commit: String,
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
    pub hash: String,
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
pub struct ArtifactWrittenPayload {
    pub path: PathBuf,
    pub content_hash: String,
    /// The artifact's declared `kind:` when it has one — what
    /// lets `derive()` recognize a `questions` artifact without reading
    /// any file (state comes from events alone). `None` for opaque
    /// artifacts and for logs written before the field existed (tolerant
    /// reader). Named `artifact_kind`, not `kind`: the event
    /// envelope's own internally-tagged discriminant already claims
    /// `kind` in the serialized JSON, and a colliding field name
    /// silently corrupts the payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_kind: Option<crate::workflow::ArtifactKind>,
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
    pub segment_hashes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskRegisteredPayload {
    pub task_id: TaskId,
    pub criteria: Vec<Criterion>,
    pub scope: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<TaskId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CriteriaCheckedPayload {
    pub task_id: TaskId,
    pub phase: Phase,
    pub results: Vec<CriterionResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskStatusChangedPayload {
    pub task_id: TaskId,
    pub new_status: TaskStatus,
    /// `seq` of the event that justifies this transition.
    pub caused_by: Seq,
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
pub struct ScopeExpansionRequestedPayload {
    pub task_id: TaskId,
    pub paths: Vec<String>,
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
    pub paths: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeFinishedPayload {
    pub outcome: String,
    pub tokens_used: TokenUsage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeFailedPayload {
    pub outcome: String,
    pub tokens_used: TokenUsage,
    pub retryable: bool,
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
    pub attempt: u32,
    pub max_reroutes: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GateWaitingPayload {
    pub summary: String,
    pub evidence: String,
    pub options: Vec<GateOption>,
    /// The forge's own handle for this gate — a PR URL,
    /// today — `None` for the internal escalation case (exhausted
    /// re-routes) this payload already covered before external
    /// gates existed. Round-trips the forge's `PublishedGate` through
    /// the log so a later `poll` (from a completely different process
    /// waking up to check on the gate) knows what to poll without
    /// re-publishing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GateResolvedPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chosen_option: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_text: Option<String>,
    /// The commit SHA the forge's approval covered —
    /// `None` for the internal escalation case, which has no SHA to
    /// speak of. What a later drift check compares against the PR's
    /// current head to decide whether the approval still holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_sha: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QuestionsAnsweredPayload {
    pub answers_hash: String,
    pub channel: Channel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LoopIterationPayload {
    pub iteration: u32,
    pub until_result: bool,
}

/// The finding's author is the envelope's own `node_id` — not repeated
/// here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingPostedPayload {
    pub finding: Finding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PromotionSignaledPayload {
    pub reason: String,
    pub evidence: String,
    pub suggested_mode: ModeName,
}

/// The parent's `kind: workflow` node is the envelope's own `node_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChildRunCreatedPayload {
    pub child_run_id: RunId,
    pub child_workflow_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChildRunFinishedPayload {
    pub child_run_id: RunId,
    pub child_workflow_hash: String,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CapabilityDegradedPayload {
    pub capability: String,
    pub adapter: AdapterId,
    pub policy_applied: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunPausedPayload {
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunResumedPayload {
    /// The one `on_interrupt` every orphan of this resume resolved to;
    /// absent when the resume found no orphan or their policies differ
    /// — `policies` is the record either way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_policy_applied: Option<String>,
    /// Every node the log left running with no terminal event, and the
    /// `on_interrupt` it resolved to: its own, or the config's default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policies: Vec<ResumePolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ResumePolicy {
    pub node: NodeId,
    pub on_interrupt: OnInterrupt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunFinishedPayload {
    pub terminal_state: TerminalState,
    pub metrics: RunMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cptv: Option<f64>,
    pub tokens: TokenUsage,
}
