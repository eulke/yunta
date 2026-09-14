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
//! thirty-seven. This enum is the same thirty-seven, flat, in the order
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
/// All 37 event kinds, internally tagged by `kind`.
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
    QuestionsAsked(QuestionsAskedPayload),
    QuestionsAnswered(QuestionsAnsweredPayload),
    LoopIteration(LoopIterationPayload),
    FindingPosted(FindingPostedPayload),
    FindingUpdated(FindingUpdatedPayload),
    FindingWithdrawn(FindingWithdrawnPayload),
    FindingRefused(FindingRefusedPayload),
    ArtifactSubmitted(ArtifactSubmittedPayload),
    ArtifactAccepted(ArtifactAcceptedPayload),
    PromotionSignaled(PromotionSignaledPayload),
    ChildRunCreated(ChildRunCreatedPayload),
    ChildRunFinished(ChildRunFinishedPayload),
    CapabilityDegraded(CapabilityDegradedPayload),
    RunPaused(RunPausedPayload),
    RunResumed(RunResumedPayload),
    RunFinished(RunFinishedPayload),
}

impl From<EventPayloadWire> for EventPayload {
    fn from(wire: EventPayloadWire) -> Self {
        use EventPayloadWire as W;
        match wire {
            W::RunCreated(p) => Self::Run(RunEvent::Created(p)),
            W::RunnerResolved(p) => Self::Node(NodeEvent::RunnerResolved(p)),
            W::BaselineCaptured(p) => Self::Node(NodeEvent::BaselineCaptured(p)),
            W::NodeStarted(p) => Self::Node(NodeEvent::Started(p)),
            W::AgentSessionOpened(p) => Self::Session(SessionEvent::Opened(p)),
            W::AgentMessage(p) => Self::Session(SessionEvent::Message(p)),
            W::ArtifactWritten(p) => Self::Artifacts(ArtifactEvent::Written(p)),
            W::ContextAssembled(p) => Self::Node(NodeEvent::ContextAssembled(p)),
            W::TaskRegistered(p) => Self::Tasks(TaskEvent::Registered(p)),
            W::CriteriaChecked(p) => Self::Node(NodeEvent::CriteriaChecked(p)),
            W::TaskStatusChanged(p) => Self::Tasks(TaskEvent::StatusChanged(p)),
            W::ScopeChecked(p) => Self::Node(NodeEvent::ScopeChecked(p)),
            W::ScopeExpansionRequested(p) => Self::Scope(ScopeEvent::Requested(p)),
            W::ScopeExpansionGranted(p) => Self::Scope(ScopeEvent::Granted(p)),
            W::ScopeExpansionDenied(p) => Self::Scope(ScopeEvent::Denied(p)),
            W::NodeFinished(p) => Self::Node(NodeEvent::Finished(p)),
            W::NodeFailed(p) => Self::Node(NodeEvent::Failed(p)),
            W::HookExecuted(p) => Self::Node(NodeEvent::HookExecuted(p)),
            W::NodeRerouted(p) => Self::Node(NodeEvent::Rerouted(p)),
            W::GateWaiting(p) => Self::Gates(GateEvent::Waiting(p)),
            W::GateResolved(p) => Self::Gates(GateEvent::Resolved(p)),
            W::QuestionsAsked(p) => Self::Gates(GateEvent::QuestionsAsked(p)),
            W::QuestionsAnswered(p) => Self::Gates(GateEvent::QuestionsAnswered(p)),
            W::LoopIteration(p) => Self::Children(ChildEvent::LoopIteration(p)),
            W::FindingPosted(p) => Self::Findings(FindingEvent::Posted(p)),
            W::FindingUpdated(p) => Self::Findings(FindingEvent::Updated(p)),
            W::FindingWithdrawn(p) => Self::Findings(FindingEvent::Withdrawn(p)),
            W::FindingRefused(p) => Self::Findings(FindingEvent::Refused(p)),
            W::ArtifactSubmitted(p) => Self::Artifacts(ArtifactEvent::Submitted(p)),
            W::ArtifactAccepted(p) => Self::Artifacts(ArtifactEvent::Accepted(p)),
            W::PromotionSignaled(p) => Self::Run(RunEvent::PromotionSignaled(p)),
            W::ChildRunCreated(p) => Self::Children(ChildEvent::Created(p)),
            W::ChildRunFinished(p) => Self::Children(ChildEvent::Finished(p)),
            W::CapabilityDegraded(p) => Self::Session(SessionEvent::CapabilityDegraded(p)),
            W::RunPaused(p) => Self::Run(RunEvent::Paused(p)),
            W::RunResumed(p) => Self::Run(RunEvent::Resumed(p)),
            W::RunFinished(p) => Self::Run(RunEvent::Finished(p)),
        }
    }
}

impl From<EventPayload> for EventPayloadWire {
    fn from(payload: EventPayload) -> Self {
        use EventPayloadWire as W;
        match payload {
            EventPayload::Run(RunEvent::Created(p)) => W::RunCreated(p),
            EventPayload::Node(NodeEvent::RunnerResolved(p)) => W::RunnerResolved(p),
            EventPayload::Node(NodeEvent::BaselineCaptured(p)) => W::BaselineCaptured(p),
            EventPayload::Node(NodeEvent::Started(p)) => W::NodeStarted(p),
            EventPayload::Session(SessionEvent::Opened(p)) => W::AgentSessionOpened(p),
            EventPayload::Session(SessionEvent::Message(p)) => W::AgentMessage(p),
            EventPayload::Artifacts(ArtifactEvent::Written(p)) => W::ArtifactWritten(p),
            EventPayload::Node(NodeEvent::ContextAssembled(p)) => W::ContextAssembled(p),
            EventPayload::Tasks(TaskEvent::Registered(p)) => W::TaskRegistered(p),
            EventPayload::Node(NodeEvent::CriteriaChecked(p)) => W::CriteriaChecked(p),
            EventPayload::Tasks(TaskEvent::StatusChanged(p)) => W::TaskStatusChanged(p),
            EventPayload::Node(NodeEvent::ScopeChecked(p)) => W::ScopeChecked(p),
            EventPayload::Scope(ScopeEvent::Requested(p)) => W::ScopeExpansionRequested(p),
            EventPayload::Scope(ScopeEvent::Granted(p)) => W::ScopeExpansionGranted(p),
            EventPayload::Scope(ScopeEvent::Denied(p)) => W::ScopeExpansionDenied(p),
            EventPayload::Node(NodeEvent::Finished(p)) => W::NodeFinished(p),
            EventPayload::Node(NodeEvent::Failed(p)) => W::NodeFailed(p),
            EventPayload::Node(NodeEvent::HookExecuted(p)) => W::HookExecuted(p),
            EventPayload::Node(NodeEvent::Rerouted(p)) => W::NodeRerouted(p),
            EventPayload::Gates(GateEvent::Waiting(p)) => W::GateWaiting(p),
            EventPayload::Gates(GateEvent::Resolved(p)) => W::GateResolved(p),
            EventPayload::Gates(GateEvent::QuestionsAsked(p)) => W::QuestionsAsked(p),
            EventPayload::Gates(GateEvent::QuestionsAnswered(p)) => W::QuestionsAnswered(p),
            EventPayload::Children(ChildEvent::LoopIteration(p)) => W::LoopIteration(p),
            EventPayload::Findings(FindingEvent::Posted(p)) => W::FindingPosted(p),
            EventPayload::Findings(FindingEvent::Updated(p)) => W::FindingUpdated(p),
            EventPayload::Findings(FindingEvent::Withdrawn(p)) => W::FindingWithdrawn(p),
            EventPayload::Findings(FindingEvent::Refused(p)) => W::FindingRefused(p),
            EventPayload::Artifacts(ArtifactEvent::Submitted(p)) => W::ArtifactSubmitted(p),
            EventPayload::Artifacts(ArtifactEvent::Accepted(p)) => W::ArtifactAccepted(p),
            EventPayload::Run(RunEvent::PromotionSignaled(p)) => W::PromotionSignaled(p),
            EventPayload::Children(ChildEvent::Created(p)) => W::ChildRunCreated(p),
            EventPayload::Children(ChildEvent::Finished(p)) => W::ChildRunFinished(p),
            EventPayload::Session(SessionEvent::CapabilityDegraded(p)) => W::CapabilityDegraded(p),
            EventPayload::Run(RunEvent::Paused(p)) => W::RunPaused(p),
            EventPayload::Run(RunEvent::Resumed(p)) => W::RunResumed(p),
            EventPayload::Run(RunEvent::Finished(p)) => W::RunFinished(p),
        }
    }
}
