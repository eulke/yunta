//! The one shape an event has on the wire, and the only place that shape
//! is written down.
//!
//! A stored event is one flat JSON object: the envelope's fields and the
//! payload's as siblings, told apart by a `kind` tag. That is what
//! `events.payload_json` holds, what `events.jsonl` carries out of the
//! system, and what the hash chain is computed over — so it cannot
//! change, whatever shape the types take in Rust.
//!
//! [`EventPayload`](super::EventPayload) is nine domain arms, because a
//! kind belongs to a domain and nothing else should have to know all
//! event kinds. This enum is the same set, flat, in the order
//! the log has always written them, and `serde` moves between the two.
//! The published JSON Schema is this enum's, which is why reorganising
//! the Rust side leaves `events.json` untouched.

use serde::{Deserialize, Serialize};

use super::artifacts::payloads::*;
use super::children::payloads::*;
use super::findings::payloads::*;
use super::gates::payloads::*;
use super::node::payloads::*;
use super::run::payloads::*;
use super::scope::payloads::*;
use super::session::payloads::*;
use super::tasks::payloads::*;
use super::{
    ArtifactEvent, ChildEvent, EventPayload, FindingEvent, GateEvent, NodeEvent, RunEvent,
    ScopeEvent, SessionEvent, TaskEvent,
};

// Never constructed by hand: this type exists so `serde` and `schemars`
// see the shape the log has while the engine sees the shape the domains
// have. Its doc comment is published in `events.json`, so it describes
// the log to whoever reads that file, not this indirection to whoever
// reads this one.
/// All event kinds, internally tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum EventPayloadWire {
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
    TaskCheckStarted(TaskCheckStartedPayload),
    TaskCheckAnswered(TaskCheckAnsweredPayload),
    DeviationDeclared(DeviationDeclaredPayload),
    DeviationResolved(DeviationResolvedPayload),
    ScopeChecked(ScopeCheckedPayload),
    ScopeExpansionRequested(ScopeExpansionRequestedPayload),
    ScopeExpansionGranted(ScopeExpansionGrantedPayload),
    ScopeExpansionDenied(ScopeExpansionDeniedPayload),
    NodeFinished(NodeFinishedPayload),
    NodeFailed(NodeFailedPayload),
    HookExecuted(HookExecutedPayload),
    NodeRerouted(NodeReroutedPayload),
    PullRequestOpened(PullRequestOpenedPayload),
    GateWaiting(GateWaitingPayload),
    GateResolved(GateResolvedPayload),
    QuestionsAsked(QuestionsAskedPayload),
    QuestionsAnswered(QuestionsAnsweredPayload),
    AskingOpened(AskingOpenedPayload),
    LoopIteration(LoopIterationPayload),
    FindingPosted(FindingPostedPayload),
    FindingUpdated(FindingUpdatedPayload),
    FindingWithdrawn(FindingWithdrawnPayload),
    FindingRefused(FindingRefusedPayload),
    FindingAnswered(FindingAnsweredPayload),
    FindingProved(FindingProvedPayload),
    FindingSettled(FindingSettledPayload),
    ArtifactSubmitted(ArtifactSubmittedPayload),
    ArtifactAccepted(ArtifactAcceptedPayload),
    PromotionSignaled(PromotionSignaledPayload),
    ChildRunCreated(ChildRunCreatedPayload),
    ChildRunFinished(ChildRunFinishedPayload),
    CapabilityDegraded(CapabilityDegradedPayload),
    WriteRefused(WriteRefusedPayload),
    RunToolFailed(RunToolFailedPayload),
    RunToolRefused(RunToolRefusedPayload),
    HostSuspended(HostSuspendedPayload),
    RunPaused(RunPausedPayload),
    RunResumed(RunResumedPayload),
    RunFinished(RunFinishedPayload),
}

/// Writes both directions between the two shapes from one table — each
/// wire variant beside the domain arm and variant it is — so they cannot
/// disagree, and a kind is added in one line.
macro_rules! wire_table {
    ($($wire:ident => $domain:ident($event:ident::$variant:ident),)*) => {
        impl From<EventPayloadWire> for EventPayload {
            fn from(wire: EventPayloadWire) -> Self {
                match wire {
                    $(EventPayloadWire::$wire(p) => Self::$domain($event::$variant(p)),)*
                }
            }
        }

        impl From<EventPayload> for EventPayloadWire {
            fn from(payload: EventPayload) -> Self {
                match payload {
                    $(EventPayload::$domain($event::$variant(p)) => Self::$wire(p),)*
                }
            }
        }
    };
}

wire_table! {
    RunCreated => Run(RunEvent::Created),
    RunnerResolved => Node(NodeEvent::RunnerResolved),
    BaselineCaptured => Run(RunEvent::BaselineCaptured),
    NodeStarted => Node(NodeEvent::Started),
    AgentSessionOpened => Session(SessionEvent::Opened),
    AgentMessage => Session(SessionEvent::Message),
    ArtifactWritten => Artifacts(ArtifactEvent::Written),
    ContextAssembled => Node(NodeEvent::ContextAssembled),
    TaskRegistered => Tasks(TaskEvent::Registered),
    CriteriaChecked => Node(NodeEvent::CriteriaChecked),
    TaskStatusChanged => Tasks(TaskEvent::StatusChanged),
    TaskCheckStarted => Tasks(TaskEvent::CheckStarted),
    TaskCheckAnswered => Tasks(TaskEvent::CheckAnswered),
    DeviationDeclared => Tasks(TaskEvent::DeviationDeclared),
    DeviationResolved => Tasks(TaskEvent::DeviationResolved),
    ScopeChecked => Node(NodeEvent::ScopeChecked),
    ScopeExpansionRequested => Scope(ScopeEvent::Requested),
    ScopeExpansionGranted => Scope(ScopeEvent::Granted),
    ScopeExpansionDenied => Scope(ScopeEvent::Denied),
    NodeFinished => Node(NodeEvent::Finished),
    NodeFailed => Node(NodeEvent::Failed),
    HookExecuted => Node(NodeEvent::HookExecuted),
    NodeRerouted => Node(NodeEvent::Rerouted),
    PullRequestOpened => Node(NodeEvent::PullRequestOpened),
    GateWaiting => Gates(GateEvent::Waiting),
    GateResolved => Gates(GateEvent::Resolved),
    QuestionsAsked => Gates(GateEvent::QuestionsAsked),
    QuestionsAnswered => Gates(GateEvent::QuestionsAnswered),
    AskingOpened => Gates(GateEvent::AskingOpened),
    LoopIteration => Children(ChildEvent::LoopIteration),
    FindingPosted => Findings(FindingEvent::Posted),
    FindingUpdated => Findings(FindingEvent::Updated),
    FindingWithdrawn => Findings(FindingEvent::Withdrawn),
    FindingRefused => Findings(FindingEvent::Refused),
    FindingAnswered => Findings(FindingEvent::Answered),
    FindingProved => Findings(FindingEvent::Proved),
    FindingSettled => Findings(FindingEvent::Settled),
    ArtifactSubmitted => Artifacts(ArtifactEvent::Submitted),
    ArtifactAccepted => Artifacts(ArtifactEvent::Accepted),
    PromotionSignaled => Run(RunEvent::PromotionSignaled),
    ChildRunCreated => Children(ChildEvent::Created),
    ChildRunFinished => Children(ChildEvent::Finished),
    CapabilityDegraded => Session(SessionEvent::CapabilityDegraded),
    WriteRefused => Session(SessionEvent::WriteRefused),
    RunToolFailed => Session(SessionEvent::RunToolFailed),
    RunToolRefused => Session(SessionEvent::RunToolRefused),
    HostSuspended => Run(RunEvent::HostSuspended),
    RunPaused => Run(RunEvent::Paused),
    RunResumed => Run(RunEvent::Resumed),
    RunFinished => Run(RunEvent::Finished),
}

/// Where an artifact came from, as the log spells it: the recorded
/// origins and `legacy` are one tagged set on the wire, while in Rust a
/// fresh acceptance can only name a [`RecordedOrigin`]. `serde` moves
/// between the two here, so the type the engine holds is not shaped by
/// the file it is written to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ArtifactOriginWire {
    Submitted,
    Ingested,
    Derived,
    Answered,
    Input {
        input: crate::ids::InputName,
    },
    Inherited {
        run: crate::ids::RunId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        producer: Option<crate::ids::NodeId>,
    },
    Legacy,
}

impl From<ArtifactOrigin> for ArtifactOriginWire {
    fn from(origin: ArtifactOrigin) -> Self {
        match origin {
            ArtifactOrigin::Recorded(RecordedOrigin::Submitted) => Self::Submitted,
            ArtifactOrigin::Recorded(RecordedOrigin::Ingested) => Self::Ingested,
            ArtifactOrigin::Recorded(RecordedOrigin::Derived) => Self::Derived,
            ArtifactOrigin::Recorded(RecordedOrigin::Answered) => Self::Answered,
            ArtifactOrigin::Recorded(RecordedOrigin::Input { input }) => Self::Input { input },
            ArtifactOrigin::Recorded(RecordedOrigin::Inherited { run, producer }) => {
                Self::Inherited { run, producer }
            }
            ArtifactOrigin::Legacy => Self::Legacy,
        }
    }
}

impl From<ArtifactOriginWire> for ArtifactOrigin {
    fn from(wire: ArtifactOriginWire) -> Self {
        match wire {
            ArtifactOriginWire::Submitted => Self::Recorded(RecordedOrigin::Submitted),
            ArtifactOriginWire::Ingested => Self::Recorded(RecordedOrigin::Ingested),
            ArtifactOriginWire::Derived => Self::Recorded(RecordedOrigin::Derived),
            ArtifactOriginWire::Answered => Self::Recorded(RecordedOrigin::Answered),
            ArtifactOriginWire::Input { input } => Self::Recorded(RecordedOrigin::Input { input }),
            ArtifactOriginWire::Inherited { run, producer } => {
                Self::Recorded(RecordedOrigin::Inherited { run, producer })
            }
            ArtifactOriginWire::Legacy => Self::Legacy,
        }
    }
}

impl Serialize for ArtifactOrigin {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ArtifactOriginWire::from(self.clone()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ArtifactOrigin {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ArtifactOriginWire::deserialize(deserializer).map(Into::into)
    }
}

impl schemars::JsonSchema for ArtifactOrigin {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ArtifactOrigin".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ArtifactOriginWire::json_schema(generator)
    }
}
