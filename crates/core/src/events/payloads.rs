//! Per-kind payload structs (`docs/eventos.md` §5). Field names and
//! optionality match that document field for field; anything the document
//! marks `[inferido]` carries the same note here.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::RunnerCandidate;
use crate::ids::{NodeId, RunId, TaskId};
use crate::Capabilities;

/// A ledger criterion, frozen into `task_registered` (`docs/spec-ledger.md`
/// §2.1) — the same shape the ledger parser (T5.1) will produce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Criterion {
    pub cmd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<CriterionType>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriterionType {
    Guard,
}

/// One criterion's outcome inside `criteria_checked.results`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CriterionResult {
    pub cmd: String,
    pub exit_code: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<CriterionType>,
    pub reused: bool,
    /// Wall-clock milliseconds this execution took (DI-15) — the datum
    /// D62's learned ordering feeds on. `None` for a `reused: true`
    /// result (nothing ran) and for pre-DI-15 events (additive, D70).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Pre,
    Post,
}

/// `[inferido]`: exact variant names are provisional pending T5.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Ready,
    Running,
    Done,
    Blocked,
    Failed,
}

/// Default `Deny` (§6.2): a node that omits `scope_expansion:` entirely
/// gets the same behavior as one that declares it with no `mode:` — no
/// expansions, every request becomes a finding without interrupting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeExpansionMode {
    Rules,
    Ask,
    #[default]
    Deny,
}

impl ScopeExpansionMode {
    /// DI-20/§6.1: the severity order the layered ceiling compares by —
    /// `rules` is the most permissive (auto-grants), `deny` the least.
    /// A higher number never grants what a lower one would refuse.
    pub fn strictness(self) -> u8 {
        match self {
            ScopeExpansionMode::Rules => 0,
            ScopeExpansionMode::Ask => 1,
            ScopeExpansionMode::Deny => 2,
        }
    }

    /// The YAML spelling, for diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeExpansionMode::Rules => "rules",
            ScopeExpansionMode::Ask => "ask",
            ScopeExpansionMode::Deny => "deny",
        }
    }
}

/// `decided_by`: `rule | person` plus an identifier for the latter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Decider {
    Rule,
    Person { id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedCriterion {
    pub cmd: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookPhase {
    Before,
    After,
}

/// One choice in a gate's escalation (§5.3): `id` is what
/// `GateResolvedPayload.chosen_option` names back, `label` is the
/// human-facing text, `tradeoff` is mandatory — "las que amplían trabajo
/// lo declaran" (§5.3's own text; there is no variant of this type that
/// can omit it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateOption {
    pub id: String,
    pub label: String,
    pub tradeoff: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Tty,
    Mcp,
    Pr,
}

/// `severity`: `blocking | major | minor | note` — confirmed against the
/// Contrato's `kind: findings` section (§4.1), not inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Blocking,
    Major,
    Minor,
    Note,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub severity: FindingSeverity,
    pub title: String,
    pub location: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_criterion: Option<ProposedCriterion>,
}

/// A `kind: findings` artifact's document — sole top-level key `findings:`
/// (§4.1), mirroring `Ledger`'s `tasks:`-only shape (T5.12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindingsFile {
    pub findings: Vec<Finding>,
}

/// `[inferido]`: exact variant names are provisional; a `cancel` command
/// exists (T7.1) so `Cancelled` is included alongside the obvious two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalState {
    Done,
    Failed,
    Cancelled,
    /// §10.2/D22: the run closed because its own gate accepted promotion
    /// to a later-declared mode — never because the work itself failed
    /// or was cancelled.
    Promoted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscardedCandidate {
    pub candidate: RunnerCandidate,
    pub reason: String,
}

/// One resolved `ContextSource` reference in `context_assembled.sources`
/// (§9, T6.1). `kind` stays a plain string — M6 fixes the *set* of
/// builtin source kinds (`files | command | artifact | run-events |
/// ledger | knowledge | node-output`, `mcp` from T6.2) but never closes
/// it into an enum, since a pack can add its own sources (M11) without
/// this type needing to change. `content_hash` is what makes "cada
/// resolución emite evento con hash" (§9) literal — the hash of exactly
/// the bytes materialized under `context/<content_hash>/` for this
/// source, so replay can name precisely what a session saw without
/// re-running anything.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextSourceRef {
    pub source_id: String,
    pub kind: String,
    pub content_hash: String,
}

// --- Per-kind payloads (docs/eventos.md §5.1-§5.25) -----------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunCreatedPayload {
    pub manifest_hash: String,
    pub inputs: HashMap<String, serde_json::Value>,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promoted_from: Option<RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yunta_schema: Option<String>,
    pub base_branch: String,
    pub base_commit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunnerResolvedPayload {
    pub role: String,
    pub chosen: RunnerCandidate,
    pub discarded: Vec<DiscardedCandidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineCapturedPayload {
    pub command: String,
    pub results: BaselineResults,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineResults {
    pub exit_code: i32,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeStartedPayload {
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSessionOpenedPayload {
    pub session_id: crate::ids::SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub model: String,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageType {
    ToolUse,
    Usage,
    Note,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactWrittenPayload {
    pub path: PathBuf,
    pub content_hash: String,
    /// The artifact's declared `kind:` when it has one (DI-03) — what
    /// lets `derive()` recognize a `questions` artifact without reading
    /// any file (I2: state from events alone). `None` for opaque
    /// artifacts and for logs written before the field existed (D70's
    /// tolerant reader). Named `artifact_kind`, not `kind`: the event
    /// envelope's own internally-tagged discriminant already claims
    /// `kind` in the serialized JSON (T2.2), and a colliding field name
    /// silently corrupts the payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_kind: Option<crate::workflow::ArtifactKind>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextAssembledPayload {
    /// `Some` when this assembly built one *task's* brief inside a loop
    /// node (DI-17) — the same task-vs-node convention
    /// `ScopeCheckedPayload.task_id` already follows. `None` for a
    /// node-level assembly (a `prompt` node's own context). Additive
    /// (D70): pre-DI-17 events parse with `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<crate::ids::TaskId>,
    pub sources: Vec<ContextSourceRef>,
    /// Keys are `"stable" | "run-stable" | "volatile"` (§9.1's fixed
    /// stability classes) — kept as plain strings rather than an enum key
    /// to sidestep serde's map-key-as-enum ceremony for no real benefit.
    pub segment_hashes: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRegisteredPayload {
    pub task_id: TaskId,
    pub criteria: Vec<Criterion>,
    pub scope: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<TaskId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CriteriaCheckedPayload {
    pub task_id: TaskId,
    pub phase: Phase,
    pub results: Vec<CriterionResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskStatusChangedPayload {
    pub task_id: TaskId,
    pub new_status: TaskStatus,
    /// `seq` of the event that justifies this transition.
    pub caused_by: u64,
}

/// `task_id` is present for a task's scope check within a loop node;
/// absent for a node-level scope check — either way the envelope's own
/// `node_id` already names the node, so it is not repeated here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopeCheckedPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    pub diff: Vec<PathBuf>,
    pub violations: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopeExpansionRequestedPayload {
    pub task_id: TaskId,
    pub paths: Vec<String>,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_criterion: Option<ProposedCriterion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_criterion_precheck: Option<ProposedCriterionPrecheck>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedCriterionPrecheck {
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopeExpansionGrantedPayload {
    pub task_id: TaskId,
    pub decided_by: Decider,
    pub mode: ScopeExpansionMode,
    pub count_this_run: u32,
    /// The exact paths this grant authorized (DI-01) — self-contained
    /// audit, and what a later attempt's effective scope derives from
    /// the log (I2), instead of re-pairing the grant with the
    /// `requested` event that preceded it. `default` for logs written
    /// before the field existed (D70's tolerant reader).
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopeExpansionDeniedPayload {
    pub task_id: TaskId,
    pub decided_by: Decider,
    pub mode: ScopeExpansionMode,
    pub count_this_run: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denial_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeFinishedPayload {
    pub outcome: String,
    pub tokens_used: TokenUsage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeFailedPayload {
    pub outcome: String,
    pub tokens_used: TokenUsage,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookExecutedPayload {
    pub phase: HookPhase,
    pub command: String,
    pub exit_code: i32,
}

/// `from_node` is the envelope's own `node_id` (the node that failed) —
/// only the re-route's destination is extra information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeReroutedPayload {
    pub to_node: NodeId,
    pub cause: String,
    pub attempt: u32,
    pub max_reroutes: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateWaitingPayload {
    pub summary: String,
    pub evidence: String,
    pub options: Vec<GateOption>,
    /// The forge's own handle for this gate (§5.6, T7.7) — a PR URL,
    /// today — `None` for the internal escalation case (exhausted
    /// re-routes, T7.2) this payload already covered before external
    /// gates existed. Round-trips the forge's `PublishedGate` through
    /// the log so a later `poll` (from a completely different process,
    /// §5.6's own "consulta al despertar") knows what to poll without
    /// re-publishing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateResolvedPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chosen_option: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_text: Option<String>,
    /// The commit SHA the forge's approval covered (§5.6, T7.7) —
    /// `None` for the internal escalation case, which has no SHA to
    /// speak of. What a later drift check compares against the PR's
    /// current head to decide whether the approval still holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_sha: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionsAnsweredPayload {
    pub answers_hash: String,
    pub channel: Channel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopIterationPayload {
    pub iteration: u32,
    pub until_result: bool,
}

/// The finding's author is the envelope's own `node_id` — not repeated
/// here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindingPostedPayload {
    pub finding: Finding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromotionSignaledPayload {
    pub reason: String,
    pub evidence: String,
    pub suggested_mode: String,
}

/// The parent's `kind: workflow` node is the envelope's own `node_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChildRunCreatedPayload {
    pub child_run_id: RunId,
    pub child_workflow_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChildRunFinishedPayload {
    pub child_run_id: RunId,
    pub child_workflow_hash: String,
    pub terminal_state: TerminalState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityDegradedPayload {
    pub capability: String,
    pub adapter: String,
    pub policy_applied: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunPausedPayload {
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunResumedPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_policy_applied: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunFinishedPayload {
    pub terminal_state: TerminalState,
    pub metrics: RunMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cptv: Option<f64>,
    pub tokens: TokenUsage,
}
