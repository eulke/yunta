//! Event log types: every `kind` the engine emits, as the event schema
//! (`crates/core/schemas/events.json`) defines them.
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

pub mod artifacts;
pub mod children;
mod evidence;
mod failure;
pub mod findings;
pub mod gates;
mod logged_words;
pub mod meta;
pub mod node;
mod redact;
mod registry;
pub mod run;
#[cfg(feature = "testkit")]
mod samples;
pub mod scope;
pub mod session;
mod spend;
pub mod tasks;
mod unknown;
mod wire;

pub use artifacts::{payloads::*, ArtifactEvent};
pub use children::{ledger::*, payloads::*, ChildEvent};
pub use evidence::{Evidence, Fact};
pub use failure::{
    AccountLimit, CapabilityUse, ChildRefusal, CommandExit, CommandOrigin, DeniedBy, DeniedCommand,
    DeniedRule, EngineError, EngineStep, Facts, Failure, FailureCause, FrozenRead, Interruption,
    LimitSource, MountError, NameProblem, ObjectProblem, Outlook, OwedScope, Precondition,
    RedCheck, RefusalCause, Remedy, RequestedScope, RunnerRefusal, SessionDeath, SessionEnd,
    SessionExit, SessionRefusal, Stuck, Timed, Unanswered, Unchanged, UnpassableTask,
    STOPPED_EXIT_CODE, TAIL_LINES,
};
pub use findings::{cause::*, payloads::*, FindingEvent};
pub use gates::{
    escalation::*, ledger::*, menu::*, offer::*, payloads::*, scope_wait::*, GateEvent,
    WhyWithheld, Withheld,
};
pub use logged_words::LoggedWords;
pub use meta::EventMeta;
pub use node::{ledger::*, payloads::*, NodeEvent};
pub use redact::Unredactable;
#[cfg(feature = "testkit")]
pub use registry::sample_payloads;
pub use registry::KIND_VERSION;
pub use run::{ledger::*, metrics::*, payloads::*, PauseReason, RunEvent, Suspension, Suspensions};
pub use scope::{ledger::*, payloads::*, ScopeEvent};
pub use session::{ledger::*, note::*, payloads::*, SessionEvent};
pub use spend::Spend;
pub use tasks::{ledger::*, payloads::*, stop::*, TaskEvent};
pub use unknown::UnknownKindCount;

// Re-exported for convenience: `agent_session_opened`'s payload uses this
// type, but it is defined at the crate root (`capabilities.rs`) since the
// `Adapter` trait shares the same definition.
pub use crate::Capabilities;

use serde::{Deserialize, Serialize};

use crate::ids::{NodeId, RunId, Seq};

/// What the engine hands to storage: what happened, in which run, for
/// which node. Storage assigns the position (`seq`) and the timestamp
/// (from the injected clock) — a draft carries neither, so no caller can
/// invent them. [`EventDraft::new`] is the only way to build one, and it
/// refuses a draft whose subject the log could not be read against.
#[derive(Debug, Clone, PartialEq)]
pub struct EventDraft {
    run_id: RunId,
    node_id: Option<NodeId>,
    payload: EventPayload,
}

/// A draft whose subject the log cannot be read against.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SubjectError {
    #[error("a grant names neither a task nor a node")]
    UngrantableSubject,
    #[error("a session fact is written with no node")]
    SessionWithoutNode,
}

impl EventDraft {
    /// Builds a draft. A grant names what it widens: a task, or the node
    /// it is written under when it has no task.
    pub fn new(
        run_id: RunId,
        node_id: Option<NodeId>,
        payload: EventPayload,
    ) -> Result<Self, SubjectError> {
        if let EventPayload::Scope(ScopeEvent::Granted(grant)) = &payload {
            if grant.task_id.is_none() && node_id.is_none() {
                return Err(SubjectError::UngrantableSubject);
            }
        }
        Ok(EventDraft {
            run_id,
            node_id,
            payload,
        })
    }

    /// The run the event belongs to.
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// The node the event is written under, if it has one.
    pub fn node_id(&self) -> Option<&NodeId> {
        self.node_id.as_ref()
    }

    /// What happened.
    pub fn payload(&self) -> &EventPayload {
        &self.payload
    }
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

    /// The schema version of the event: [`KIND_VERSION`] for a kind this
    /// binary knows, the version it was stored with for one it does not.
    pub fn schema_version(&self) -> u32 {
        match self {
            EventBody::Known(_) => KIND_VERSION,
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

/// What happened, in the terms of the domain it happened to.
///
/// Nine arms, one per domain; each domain declares its own kinds and
/// their payloads. Nothing outside a domain has to know every kind, and a
/// kind's name and sample are its row in the registry.
///
/// On the wire this is still one flat object tagged by `kind`: `serde`
/// goes through a private flat enum that the registry generates, in the
/// order the log has always written its kinds, so the shape a log carries
/// is independent of the shape the engine reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    from = "registry::EventPayloadWire",
    into = "registry::EventPayloadWire"
)]
pub enum EventPayload {
    Run(RunEvent),
    Node(NodeEvent),
    Session(SessionEvent),
    Tasks(TaskEvent),
    Scope(ScopeEvent),
    Findings(FindingEvent),
    Artifacts(ArtifactEvent),
    Gates(GateEvent),
    Children(ChildEvent),
}

/// The schema published for an event payload is the wire shape's: a
/// `oneOf` with one branch per kind, each pinning its own `kind`, in the
/// order the log writes them. The nine domains are an internal shape and
/// no reader of `events.json` ever learns about them.
impl schemars::JsonSchema for EventPayload {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("EventPayload")
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        registry::EventPayloadWire::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        registry::EventPayloadWire::json_schema(generator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::ScopeExpansionMode;

    #[test]
    fn a_grant_naming_no_subject_is_refused_at_the_draft() {
        let grant = ScopeEvent::Granted(ScopeExpansionGrantedPayload {
            task_id: None,
            decided_by: Decider::Rule,
            mode: Some(ScopeExpansionMode::Rules),
            count_this_run: 1,
            paths: vec![],
        });
        let draft = EventDraft::new(RunId::from("run-1"), None, EventPayload::Scope(grant));
        assert!(matches!(draft, Err(SubjectError::UngrantableSubject)));
    }
}
