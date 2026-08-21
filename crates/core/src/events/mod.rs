//! Event log types — the full 31 `kind`s from `docs/eventos.md`,
//! covering every kind even though
//! the current engine (only `prompt`/`bash`/`loop` nodes exist so far) won't
//! emit most of them until gates, `parallel`,
//! composition and scope expansion land in the schema.
//!
//! Versioning: `schema_version` is
//! per `kind`, not global. Every kind starts at v1 here — there is no
//! prior history. An incompatible change to a payload becomes a new
//! `kind` name (e.g. `criteria_checked_v2`), never a migration of this
//! enum's existing variant — the log is append-only, so an existing
//! variant's shape never changes underneath it.
//!
//! `event_hash` is deliberately **not** implemented here yet — the
//! hash-chain policy is already decided, so building it is only a matter
//! of wiring it in.

mod payloads;

pub use payloads::*;

// Re-exported for convenience: `agent_session_opened`'s payload uses this
// type, but it is defined at the crate root (`capabilities.rs`) since the
// `Adapter` trait shares the same definition.
pub use crate::Capabilities;

use serde::{Deserialize, Serialize};

use crate::ids::{NodeId, RunId};

/// One event as persisted.
/// `kind` and `schema_version` are not separate fields here: `payload`'s
/// enum tag already carries the `kind` name (see [`EventPayload::kind_name`]),
/// and every payload is currently at v1 so a `schema_version` field
/// would be a constant — storage is where the literal `kind`
/// string and `schema_version` column get written from
/// [`EventPayload::kind_name`] and [`EventPayload::schema_version`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub run_id: RunId,
    pub seq: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<NodeId>,
    #[serde(flatten)]
    pub payload: EventPayload,
}

/// All 31 event kinds, internally tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventPayload {
    RunCreated(RunCreatedPayload),
    RunnerResolved(RunnerResolvedPayload),
    BaselineCaptured(BaselineCapturedPayload),
    NodeStarted(NodeStartedPayload),
    AgentSessionOpened(AgentSessionOpenedPayload),
    AgentMessage(AgentMessagePayload),
    ArtifactWritten(ArtifactWrittenPayload),
    ContextAssembled(ContextAssembledPayload),
    TaskRegistered(TaskRegisteredPayload),
    CriteriaChecked(CriteriaCheckedPayload),
    TaskStatusChanged(TaskStatusChangedPayload),
    ScopeChecked(ScopeCheckedPayload),
    ScopeExpansionRequested(ScopeExpansionRequestedPayload),
    ScopeExpansionGranted(ScopeExpansionGrantedPayload),
    ScopeExpansionDenied(ScopeExpansionDeniedPayload),
    NodeFinished(NodeFinishedPayload),
    NodeFailed(NodeFailedPayload),
    HookExecuted(HookExecutedPayload),
    NodeRerouted(NodeReroutedPayload),
    GateWaiting(GateWaitingPayload),
    GateResolved(GateResolvedPayload),
    QuestionsAnswered(QuestionsAnsweredPayload),
    LoopIteration(LoopIterationPayload),
    FindingPosted(FindingPostedPayload),
    PromotionSignaled(PromotionSignaledPayload),
    ChildRunCreated(ChildRunCreatedPayload),
    ChildRunFinished(ChildRunFinishedPayload),
    CapabilityDegraded(CapabilityDegradedPayload),
    RunPaused(RunPausedPayload),
    RunResumed(RunResumedPayload),
    RunFinished(RunFinishedPayload),
}

/// The mode `run_created` froze for this log — always the
/// log's own first event; `"default"` for a log without one (pre-modes,
/// or truncated). This is the one place that mode is derived: `execute_run`'s
/// resume and the stats surfaces must never disagree about a run's mode.
pub fn run_mode(events: &[Event]) -> &str {
    match events.first().map(|event| &event.payload) {
        Some(EventPayload::RunCreated(p)) => &p.mode,
        _ => "default",
    }
}

impl EventPayload {
    /// The persisted `kind` string — what storage writes to its
    /// `kind` column, independent of re-serializing the whole payload.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::RunCreated(_) => "run_created",
            Self::RunnerResolved(_) => "runner_resolved",
            Self::BaselineCaptured(_) => "baseline_captured",
            Self::NodeStarted(_) => "node_started",
            Self::AgentSessionOpened(_) => "agent_session_opened",
            Self::AgentMessage(_) => "agent_message",
            Self::ArtifactWritten(_) => "artifact_written",
            Self::ContextAssembled(_) => "context_assembled",
            Self::TaskRegistered(_) => "task_registered",
            Self::CriteriaChecked(_) => "criteria_checked",
            Self::TaskStatusChanged(_) => "task_status_changed",
            Self::ScopeChecked(_) => "scope_checked",
            Self::ScopeExpansionRequested(_) => "scope_expansion_requested",
            Self::ScopeExpansionGranted(_) => "scope_expansion_granted",
            Self::ScopeExpansionDenied(_) => "scope_expansion_denied",
            Self::NodeFinished(_) => "node_finished",
            Self::NodeFailed(_) => "node_failed",
            Self::HookExecuted(_) => "hook_executed",
            Self::NodeRerouted(_) => "node_rerouted",
            Self::GateWaiting(_) => "gate_waiting",
            Self::GateResolved(_) => "gate_resolved",
            Self::QuestionsAnswered(_) => "questions_answered",
            Self::LoopIteration(_) => "loop_iteration",
            Self::FindingPosted(_) => "finding_posted",
            Self::PromotionSignaled(_) => "promotion_signaled",
            Self::ChildRunCreated(_) => "child_run_created",
            Self::ChildRunFinished(_) => "child_run_finished",
            Self::CapabilityDegraded(_) => "capability_degraded",
            Self::RunPaused(_) => "run_paused",
            Self::RunResumed(_) => "run_resumed",
            Self::RunFinished(_) => "run_finished",
        }
    }

    /// Every kind is currently at v1 — this is a
    /// stub now, and becomes real per-variant lookup the day any kind
    /// gets an incompatible `_v2` sibling.
    pub fn schema_version(&self) -> u32 {
        1
    }
}
