//! Per-kind payload structs. Field names and
//! optionality match the reference event documentation field for field;
//! anything provisional there carries the same note here.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::RunnerCandidate;
use crate::events::Failure;
use crate::hash::{CommitSha, ContentHash};
use crate::ids::{
    AdapterId, AgentName, FindingId, ModeName, ModelName, NodeId, OptionId, Responder, RunId,
    RunnerName, Seq, SessionId, TaskId,
};
use crate::policy::ScopeExpansionMode;
use crate::workflow::OnInterrupt;
use crate::{Capabilities, Capability};

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
    Person { id: Responder },
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
    pub id: OptionId,
    pub label: String,
    pub tradeoff: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Tty,
    Mcp,
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

impl FindingSeverity {
    /// The ladder, as a document writes it. Tied to what serde derives
    /// by a test, so a diagnostic listing the ladder cannot list a
    /// different one from the parser accepting it.
    pub const NAMES: [&'static str; 4] = ["blocking", "major", "minor", "note"];
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
    pub content_hash: ContentHash,
}

// --- Per-kind payloads ------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunCreatedPayload {
    pub manifest_hash: ContentHash,
    pub inputs: BTreeMap<String, serde_json::Value>,
    pub mode: ModeName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promoted_from: Option<RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yunta_schema: Option<String>,
    pub base_branch: String,
    pub base_commit: CommitSha,
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
    pub content_hash: ContentHash,
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
    pub segment_hashes: BTreeMap<String, ContentHash>,
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

impl GateWaitingPayload {
    /// Whether `option` is on this escalation's menu: the one test an
    /// answer passes before it counts as a decision on it.
    pub fn offers(&self, option: &OptionId) -> bool {
        self.options.iter().any(|o| o.id == *option)
    }

    /// The menu's option ids as one comma-separated line, for a message
    /// that names what was offered.
    pub fn menu(&self) -> String {
        self.options
            .iter()
            .map(|o| o.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// How a gate's escalation was settled. On the wire this is one flat
/// object of four optional fields, and the fields present spell the
/// shape: an option with its responder is a human's `Chosen`; a
/// responder with a commit id is the forge's `Approved`; a responder
/// alone is `ChangesRequested`; nothing at all is `Closed`. Reading
/// decides the shape once, here, so every reader matches on it instead
/// of inferring it from which field is set. A combination no shape
/// names reads as `Unrecognized` and writes back verbatim: a newer
/// writer may mean something by it, and the export loses nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(from = "GateResolvedWire", into = "GateResolvedWire")]
pub enum GateResolvedPayload {
    /// A human picked one of the escalation's options: an internal
    /// gate, exhausted re-routes, a token budget, a scope expansion, or
    /// an external gate degraded to the console.
    Chosen(HumanChoice),
    /// The forge reports an approving review, or a merge, covering
    /// `sha`. A merge is an approval whose evidence is the merge commit.
    Approved { by: Responder, sha: CommitSha },
    /// The forge reports a changes-requested review by `by`.
    ChangesRequested { by: Responder },
    /// The pull request was closed without merging.
    Closed,
    /// A combination of fields no shape above names, kept as read. Only
    /// reading produces it; nothing in this workspace writes one.
    Unrecognized(UnrecognizedResolution),
}

/// One option picked from an escalation's menu, and who picked it: the
/// content of [`GateResolvedPayload::Chosen`], and the only shape a
/// human-facing surface produces. A console or an MCP tool chooses; it
/// never reports an approval a forge did not give.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanChoice {
    pub option: OptionId,
    pub by: Responder,
    pub free_text: Option<String>,
}

/// A `gate_resolved` whose fields spell no shape this binary names.
/// Opaque: it exists to be written back unchanged, never to be read
/// into a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnrecognizedResolution(GateResolvedWire);

/// The persisted object behind [`GateResolvedPayload`]: four optional
/// fields, the same for every shape. Serialization, deserialization and
/// the JSON Schema all go through it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
struct GateResolvedWire {
    /// The option a human chose from the menu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chosen_option: Option<OptionId>,
    /// Who decided: the human who chose, or the reviewer or merger the
    /// forge reports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resolved_by: Option<Responder>,
    /// Free-form context a human gave alongside the choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    free_text: Option<String>,
    /// The commit the forge's approval covers: what a later drift check
    /// compares against the pull request's current head to decide
    /// whether the approval still holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    approved_sha: Option<CommitSha>,
}

impl From<GateResolvedWire> for GateResolvedPayload {
    fn from(wire: GateResolvedWire) -> Self {
        match wire {
            GateResolvedWire {
                chosen_option: Some(option),
                resolved_by: Some(by),
                free_text,
                approved_sha: None,
            } => Self::Chosen(HumanChoice {
                option,
                by,
                free_text,
            }),
            GateResolvedWire {
                chosen_option: None,
                resolved_by: Some(by),
                free_text: None,
                approved_sha: Some(sha),
            } => Self::Approved { by, sha },
            GateResolvedWire {
                chosen_option: None,
                resolved_by: Some(by),
                free_text: None,
                approved_sha: None,
            } => Self::ChangesRequested { by },
            GateResolvedWire {
                chosen_option: None,
                resolved_by: None,
                free_text: None,
                approved_sha: None,
            } => Self::Closed,
            other => Self::Unrecognized(UnrecognizedResolution(other)),
        }
    }
}

impl From<GateResolvedPayload> for GateResolvedWire {
    fn from(payload: GateResolvedPayload) -> Self {
        match payload {
            GateResolvedPayload::Chosen(HumanChoice {
                option,
                by,
                free_text,
            }) => GateResolvedWire {
                chosen_option: Some(option),
                resolved_by: Some(by),
                free_text,
                approved_sha: None,
            },
            GateResolvedPayload::Approved { by, sha } => GateResolvedWire {
                resolved_by: Some(by),
                approved_sha: Some(sha),
                ..GateResolvedWire::default()
            },
            GateResolvedPayload::ChangesRequested { by } => GateResolvedWire {
                resolved_by: Some(by),
                ..GateResolvedWire::default()
            },
            GateResolvedPayload::Closed => GateResolvedWire::default(),
            GateResolvedPayload::Unrecognized(UnrecognizedResolution(wire)) => wire,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QuestionsAnsweredPayload {
    pub answers_hash: ContentHash,
    pub channel: Channel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responder: Option<Responder>,
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

/// The node's finding, in its new state — a whole replacement, never a
/// merge: a field absent from an update is absent from the finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingUpdatedPayload {
    pub finding: Finding,
}

/// The node's finding `id` no longer stands, and why — in the words of
/// whoever took it back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingWithdrawnPayload {
    pub id: FindingId,
    pub reason: String,
}

/// Which of the three a refused call was making.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindingOperation {
    Post,
    Update,
    Withdraw,
}

/// A finding a session offered and the engine did not take, with every
/// problem named. The accepted cases are the three events above; this is
/// what a session was told instead, kept so the rate a run gets findings
/// wrong is a fact about the run rather than something only the session
/// saw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingRefusedPayload {
    pub operation: FindingOperation,
    /// The id the call named, when it named one that parses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<FindingId>,
    pub report: crate::diagnostic::Report,
}

/// What a session offered as a whole document, and what the engine
/// answered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactSubmittedPayload {
    pub name: String,
    /// Named `artifact_kind`, not `kind`, for the reason
    /// [`ArtifactWrittenPayload`] gives: the envelope's own tag already
    /// claims `kind` in the serialized JSON.
    pub artifact_kind: crate::workflow::ArtifactKind,
    pub outcome: SubmissionOutcome,
}

/// Accepted, and the file the engine wrote from it; or refused, and
/// every problem the document has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionOutcome {
    Accepted {
        content_hash: crate::hash::ContentHash,
    },
    Refused {
        report: crate::diagnostic::Report,
    },
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CapabilityDegradedPayload {
    /// The capability the engine consulted and the adapter does not declare.
    pub capability: Capability,
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
