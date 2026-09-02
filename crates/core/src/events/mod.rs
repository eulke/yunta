//! Event log types: every `kind` the engine emits, as the event schema
//! (`docs/design/spec-events.md`) defines them.
//!
//! Versioning: `schema_version` is
//! per `kind`, not global. Every kind starts at v1 here — there is no
//! prior history. An incompatible change to a payload becomes a new
//! `kind` name (e.g. `criteria_checked_v2`), never a migration of this
//! enum's existing variant — the log is append-only, so an existing
//! variant's shape never changes underneath it.
//!
//! The hash chain over persisted rows lives in `yunta-storage`; nothing
//! here carries a hash.

mod payloads;

pub use payloads::*;

// Re-exported for convenience: `agent_session_opened`'s payload uses this
// type, but it is defined at the crate root (`capabilities.rs`) since the
// `Adapter` trait shares the same definition.
pub use crate::Capabilities;

use serde::{Deserialize, Serialize};

use crate::ids::{ModeName, NodeId, RunId, Seq};

/// What the engine hands to storage: what happened, in which run, for
/// which node. Storage assigns the position (`seq`) and the timestamp
/// (from the injected clock) — a draft carries neither, so no caller can
/// invent them.
#[derive(Debug, Clone, PartialEq)]
pub struct EventDraft {
    pub run_id: RunId,
    pub node_id: Option<NodeId>,
    pub payload: EventPayload,
}

/// One event as persisted and read back. On the wire (`events.jsonl`,
/// the database row) the envelope fields sit beside the body's own:
/// `{run_id, seq, timestamp, node_id?, kind, ...payload}`.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    pub run_id: RunId,
    pub seq: Seq,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub node_id: Option<NodeId>,
    pub body: EventBody,
}

impl StoredEvent {
    /// The payload when this binary knows the event's kind; `None` for a
    /// kind it does not — the event still counts, replay just cannot
    /// interpret it.
    pub fn payload(&self) -> Option<&EventPayload> {
        match &self.body {
            EventBody::Known(payload) => Some(payload),
            EventBody::Unknown(_) => None,
        }
    }
}

/// A stored event's content: a payload this binary knows, or one written
/// under a `kind` it does not. The reader keeps the unknown one verbatim
/// (the spec's rule: a run with an unknown kind is partially interpreted,
/// never broken), and re-serializes it untouched.
#[derive(Debug, Clone, PartialEq)]
pub enum EventBody {
    Known(EventPayload),
    Unknown(UnknownEvent),
}

impl EventBody {
    /// The persisted `kind` string, whether or not this binary knows it.
    pub fn kind_name(&self) -> &str {
        match self {
            EventBody::Known(payload) => payload.kind_name(),
            EventBody::Unknown(unknown) => &unknown.kind,
        }
    }

    /// The version the event was written under.
    pub fn schema_version(&self) -> u32 {
        match self {
            EventBody::Known(payload) => payload.schema_version(),
            EventBody::Unknown(unknown) => unknown.schema_version,
        }
    }
}

/// An event under a `kind` this binary does not know, kept as written:
/// the kind, the version it was written under, and every field of the
/// object except the kind tag.
#[derive(Debug, Clone, PartialEq)]
pub struct UnknownEvent {
    pub kind: String,
    pub schema_version: u32,
    pub payload: serde_json::Map<String, serde_json::Value>,
}

/// An event object that does not have the shape of an event.
#[derive(Debug, thiserror::Error)]
pub enum EventShapeError {
    #[error("the payload is not a JSON object")]
    NotAnObject,
    #[error("the payload is not JSON")]
    Json {
        #[source]
        source: serde_json::Error,
    },
    #[error("an event names its `kind` as a string")]
    KindMissing,
    #[error("the fields of a `{kind}` event do not fit its shape")]
    Payload {
        kind: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("a `{kind}` event does not serialize as an object")]
    Serialize {
        kind: String,
        #[source]
        source: serde_json::Error,
    },
}

impl UnknownEvent {
    /// Reads an event object whose `kind` this binary does not know.
    /// `object` holds the whole body — the kind tag and the fields beside
    /// it; the tag is kept as `kind`, the rest as the payload.
    pub fn from_object(
        mut object: serde_json::Map<String, serde_json::Value>,
        schema_version: u32,
    ) -> Result<Self, EventShapeError> {
        let kind = match object.remove("kind") {
            Some(serde_json::Value::String(kind)) => kind,
            _ => return Err(EventShapeError::KindMissing),
        };
        Ok(UnknownEvent {
            kind,
            schema_version,
            payload: object,
        })
    }

    /// The body as one object, with the kind tag back in its place.
    pub fn to_object(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut object = serde_json::Map::with_capacity(self.payload.len() + 1);
        object.insert(
            "kind".to_string(),
            serde_json::Value::String(self.kind.clone()),
        );
        object.extend(self.payload.iter().map(|(k, v)| (k.clone(), v.clone())));
        object
    }
}

impl EventBody {
    /// Reads a body from its JSON object: the payload when `kind` is
    /// known (a known kind whose fields do not fit is an error, never an
    /// unknown), the object kept verbatim otherwise.
    pub fn from_object(
        object: serde_json::Map<String, serde_json::Value>,
        schema_version: u32,
    ) -> Result<Self, EventShapeError> {
        let kind = object
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or(EventShapeError::KindMissing)?
            .to_string();
        if EventPayload::KINDS.contains(&kind.as_str()) {
            serde_json::from_value(serde_json::Value::Object(object))
                .map(EventBody::Known)
                .map_err(|source| EventShapeError::Payload { kind, source })
        } else {
            UnknownEvent::from_object(object, schema_version).map(EventBody::Unknown)
        }
    }

    /// The body as one JSON object, the kind tag included.
    pub fn to_object(&self) -> Result<serde_json::Map<String, serde_json::Value>, EventShapeError> {
        match self {
            EventBody::Known(payload) => match serde_json::to_value(payload) {
                Ok(serde_json::Value::Object(object)) => Ok(object),
                Ok(_) => Err(EventShapeError::NotAnObject),
                Err(source) => Err(EventShapeError::Serialize {
                    kind: self.kind_name().to_string(),
                    source,
                }),
            },
            EventBody::Unknown(unknown) => Ok(unknown.to_object()),
        }
    }
}

/// The wire shape's envelope fields. Kept apart from [`StoredEvent`] so
/// the body can be flattened beside them by hand: serde's own
/// `flatten` cannot fall back to an unknown kind.
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct Envelope {
    run_id: RunId,
    seq: Seq,
    timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    node_id: Option<NodeId>,
    /// Present only for an unknown kind, whose version has no other
    /// place to live on the wire; a known kind's version is its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    schema_version: Option<u32>,
}

impl Serialize for StoredEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error;

        let envelope = Envelope {
            run_id: self.run_id.clone(),
            seq: self.seq,
            timestamp: self.timestamp,
            node_id: self.node_id.clone(),
            schema_version: match &self.body {
                EventBody::Known(_) => None,
                EventBody::Unknown(unknown) => Some(unknown.schema_version),
            },
        };
        let mut object = match serde_json::to_value(&envelope).map_err(S::Error::custom)? {
            serde_json::Value::Object(object) => object,
            _ => return Err(S::Error::custom("the envelope serializes as an object")),
        };
        object.extend(self.body.to_object().map_err(S::Error::custom)?);
        serde_json::Value::Object(object).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StoredEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        let mut object = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let mut envelope_fields = serde_json::Map::new();
        for field in ["run_id", "seq", "timestamp", "node_id", "schema_version"] {
            if let Some(value) = object.remove(field) {
                envelope_fields.insert(field.to_string(), value);
            }
        }
        let envelope: Envelope = serde_json::from_value(serde_json::Value::Object(envelope_fields))
            .map_err(D::Error::custom)?;
        let body = EventBody::from_object(object, envelope.schema_version.unwrap_or(1))
            .map_err(D::Error::custom)?;
        Ok(StoredEvent {
            run_id: envelope.run_id,
            seq: envelope.seq,
            timestamp: envelope.timestamp,
            node_id: envelope.node_id,
            body,
        })
    }
}

impl schemars::JsonSchema for StoredEvent {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "StoredEvent".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "One event of a run's log: the envelope and the payload of its \
                            `kind`, side by side in one object",
            "allOf": [
                generator.subschema_for::<Envelope>(),
                generator.subschema_for::<EventPayload>()
            ]
        })
    }
}

/// All 31 event kinds, internally tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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

/// The mode `run_created` froze for this log — always the log's own
/// first event; the default mode for a log without one (written before
/// modes existed, or truncated). This is the one place that mode is
/// derived: `execute_run`'s resume and the stats surfaces must never
/// disagree about a run's mode.
pub fn run_mode(events: &[StoredEvent]) -> ModeName {
    match events.first().and_then(StoredEvent::payload) {
        Some(EventPayload::RunCreated(p)) => p.mode.clone(),
        _ => ModeName::default(),
    }
}

impl EventPayload {
    /// Every kind this binary knows, as persisted — what the reader checks
    /// a stored `kind` against before deciding it is unknown.
    pub const KINDS: &'static [&'static str] = &[
        "run_created",
        "runner_resolved",
        "baseline_captured",
        "node_started",
        "agent_session_opened",
        "agent_message",
        "artifact_written",
        "context_assembled",
        "task_registered",
        "criteria_checked",
        "task_status_changed",
        "scope_checked",
        "scope_expansion_requested",
        "scope_expansion_granted",
        "scope_expansion_denied",
        "node_finished",
        "node_failed",
        "hook_executed",
        "node_rerouted",
        "gate_waiting",
        "gate_resolved",
        "questions_answered",
        "loop_iteration",
        "finding_posted",
        "promotion_signaled",
        "child_run_created",
        "child_run_finished",
        "capability_degraded",
        "run_paused",
        "run_resumed",
        "run_finished",
    ];

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
