//! The event registry: one row per kind.
//!
//! A row names a kind's wire name, its payload, the domain variant it is
//! read into and the sample every test builds it from. The wire enum, the
//! kind names, both conversions between the wire shape and the domain
//! shape, and the sample set all come from these rows, so a kind added
//! without a sample does not compile.

use serde::{Deserialize, Serialize};

use super::artifacts::payloads::*;
use super::children::payloads::*;
use super::findings::payloads::*;
use super::gates::payloads::*;
use super::node::payloads::*;
use super::run::payloads::*;
#[cfg(feature = "testkit")]
use super::samples;
use super::scope::payloads::*;
use super::session::payloads::*;
use super::tasks::payloads::*;
use super::{
    ArtifactEvent, ChildEvent, EventPayload, FindingEvent, GateEvent, NodeEvent, RunEvent,
    ScopeEvent, SessionEvent, TaskEvent,
};

/// One row per kind: wire name, payload, domain variant and the sample every test builds it from.
macro_rules! event_kinds {
    ($( $wire:ident($payload:ty) = $name:literal => $domain:ident($event:ident::$variant:ident)
        sample $sample:path; )*) => {
        /// All event kinds, internally tagged by `kind`; published in `events.json`.
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
        #[serde(tag = "kind")]
        pub(crate) enum EventPayloadWire { $( #[serde(rename = $name)] $wire($payload), )* }
        impl EventPayload {
            /// Every kind this binary knows, as persisted: what a reader checks a
            /// stored `kind` against before deciding it is unknown.
            pub const KINDS: &'static [&'static str] = &[$($name),*];
            /// The persisted `kind` string: what storage writes to its `kind` column.
            pub fn kind_name(&self) -> &'static str {
                match self { $( Self::$domain($event::$variant(_)) => $name, )* }
            }
        }
        /// One sample of every kind, in log order (`testkit` builds only).
        #[cfg(feature = "testkit")]
        pub fn sample_payloads() -> Vec<EventPayload> {
            vec![$( EventPayload::$domain($event::$variant($sample())), )*]
        }
        impl From<EventPayloadWire> for EventPayload {
            fn from(wire: EventPayloadWire) -> Self {
                match wire {
                    $( EventPayloadWire::$wire(p) => Self::$domain($event::$variant(p)), )*
                }
            }
        }
        impl From<EventPayload> for EventPayloadWire {
            fn from(payload: EventPayload) -> Self {
                match payload {
                    $( EventPayload::$domain($event::$variant(p)) => Self::$wire(p), )*
                }
            }
        }
    };
}

/// Every kind is written at version 1. A changed shape is a new name, never a bump.
pub const KIND_VERSION: u32 = 1;

event_kinds! {
    RunCreated(RunCreatedPayload) = "run_created"
        => Run(RunEvent::Created) sample samples::run_created;
    RunnerResolved(RunnerResolvedPayload) = "runner_resolved"
        => Node(NodeEvent::RunnerResolved) sample samples::runner_resolved;
    BaselineCaptured(BaselineCapturedPayload) = "baseline_captured"
        => Run(RunEvent::BaselineCaptured) sample samples::baseline_captured;
    BaselineHeld(BaselineHeldPayload) = "baseline_held"
        => Run(RunEvent::BaselineHeld) sample samples::baseline_held;
    NodeStarted(NodeStartedPayload) = "node_started"
        => Node(NodeEvent::Started) sample samples::node_started;
    AgentSessionOpened(AgentSessionOpenedPayload) = "agent_session_opened"
        => Session(SessionEvent::Opened) sample samples::agent_session_opened;
    AgentMessage(AgentMessagePayload) = "agent_message"
        => Session(SessionEvent::Message) sample samples::agent_message;
    ArtifactWritten(ArtifactWrittenPayload) = "artifact_written"
        => Artifacts(ArtifactEvent::Written) sample samples::artifact_written;
    ContextAssembled(ContextAssembledPayload) = "context_assembled"
        => Node(NodeEvent::ContextAssembled) sample samples::context_assembled;
    TaskRegistered(TaskRegisteredPayload) = "task_registered"
        => Tasks(TaskEvent::Registered) sample samples::task_registered;
    CriteriaChecked(CriteriaCheckedPayload) = "criteria_checked"
        => Node(NodeEvent::CriteriaChecked) sample samples::criteria_checked;
    TaskStatusChanged(TaskStatusChangedPayload) = "task_status_changed"
        => Tasks(TaskEvent::StatusChanged) sample samples::task_status_changed;
    TaskCheckStarted(TaskCheckStartedPayload) = "task_check_started"
        => Tasks(TaskEvent::CheckStarted) sample samples::task_check_started;
    TaskCheckAnswered(TaskCheckAnsweredPayload) = "task_check_answered"
        => Tasks(TaskEvent::CheckAnswered) sample samples::task_check_answered;
    DeviationDeclared(DeviationDeclaredPayload) = "deviation_declared"
        => Tasks(TaskEvent::DeviationDeclared) sample samples::deviation_declared;
    DeviationResolved(DeviationResolvedPayload) = "deviation_resolved"
        => Tasks(TaskEvent::DeviationResolved) sample samples::deviation_resolved;
    ScopeChecked(ScopeCheckedPayload) = "scope_checked"
        => Node(NodeEvent::ScopeChecked) sample samples::scope_checked;
    ScopeExpansionRequested(ScopeExpansionRequestedPayload) = "scope_expansion_requested"
        => Scope(ScopeEvent::Requested) sample samples::scope_expansion_requested;
    ScopeExpansionGranted(ScopeExpansionGrantedPayload) = "scope_expansion_granted"
        => Scope(ScopeEvent::Granted) sample samples::scope_expansion_granted;
    ScopeExpansionDenied(ScopeExpansionDeniedPayload) = "scope_expansion_denied"
        => Scope(ScopeEvent::Denied) sample samples::scope_expansion_denied;
    ScopeDerived(ScopeDerivedPayload) = "scope_derived"
        => Scope(ScopeEvent::Derived) sample samples::scope_derived;
    NodeFinished(NodeFinishedPayload) = "node_finished"
        => Node(NodeEvent::Finished) sample samples::node_finished;
    NodeFailed(NodeFailedPayload) = "node_failed"
        => Node(NodeEvent::Failed) sample samples::node_failed;
    HookExecuted(HookExecutedPayload) = "hook_executed"
        => Node(NodeEvent::HookExecuted) sample samples::hook_executed;
    NodeRerouted(NodeReroutedPayload) = "node_rerouted"
        => Node(NodeEvent::Rerouted) sample samples::node_rerouted;
    PullRequestOpened(PullRequestOpenedPayload) = "pull_request_opened"
        => Node(NodeEvent::PullRequestOpened) sample samples::pull_request_opened;
    GateWaiting(GateWaitingPayload) = "gate_waiting"
        => Gates(GateEvent::Waiting) sample samples::gate_waiting;
    GateResolved(GateResolvedPayload) = "gate_resolved"
        => Gates(GateEvent::Resolved) sample samples::gate_resolved;
    QuestionsAsked(QuestionsAskedPayload) = "questions_asked"
        => Gates(GateEvent::QuestionsAsked) sample samples::questions_asked;
    QuestionsAnswered(QuestionsAnsweredPayload) = "questions_answered"
        => Gates(GateEvent::QuestionsAnswered) sample samples::questions_answered;
    AskingOpened(AskingOpenedPayload) = "asking_opened"
        => Gates(GateEvent::AskingOpened) sample samples::asking_opened;
    LoopIteration(LoopIterationPayload) = "loop_iteration"
        => Children(ChildEvent::LoopIteration) sample samples::loop_iteration;
    FindingPosted(FindingPostedPayload) = "finding_posted"
        => Findings(FindingEvent::Posted) sample samples::finding_posted;
    FindingUpdated(FindingUpdatedPayload) = "finding_updated"
        => Findings(FindingEvent::Updated) sample samples::finding_updated;
    FindingWithdrawn(FindingWithdrawnPayload) = "finding_withdrawn"
        => Findings(FindingEvent::Withdrawn) sample samples::finding_withdrawn;
    FindingRefused(FindingRefusedPayload) = "finding_refused"
        => Findings(FindingEvent::Refused) sample samples::finding_refused;
    FindingAnswered(FindingAnsweredPayload) = "finding_answered"
        => Findings(FindingEvent::Answered) sample samples::finding_answered;
    FindingProved(FindingProvedPayload) = "finding_proved"
        => Findings(FindingEvent::Proved) sample samples::finding_proved;
    FindingSettled(FindingSettledPayload) = "finding_settled"
        => Findings(FindingEvent::Settled) sample samples::finding_settled;
    ArtifactSubmitted(ArtifactSubmittedPayload) = "artifact_submitted"
        => Artifacts(ArtifactEvent::Submitted) sample samples::artifact_submitted;
    ArtifactAccepted(ArtifactAcceptedPayload) = "artifact_accepted"
        => Artifacts(ArtifactEvent::Accepted) sample samples::artifact_accepted;
    PromotionSignaled(PromotionSignaledPayload) = "promotion_signaled"
        => Run(RunEvent::PromotionSignaled) sample samples::promotion_signaled;
    ChildRunCreated(ChildRunCreatedPayload) = "child_run_created"
        => Children(ChildEvent::Created) sample samples::child_run_created;
    ChildRunFinished(ChildRunFinishedPayload) = "child_run_finished"
        => Children(ChildEvent::Finished) sample samples::child_run_finished;
    CapabilityDegraded(CapabilityDegradedPayload) = "capability_degraded"
        => Session(SessionEvent::CapabilityDegraded) sample samples::capability_degraded;
    WriteRefused(WriteRefusedPayload) = "write_refused"
        => Session(SessionEvent::WriteRefused) sample samples::write_refused;
    StopRefused(StopRefusedPayload) = "stop_refused"
        => Session(SessionEvent::StopRefused) sample samples::stop_refused;
    WriteFormatted(WriteFormattedPayload) = "write_formatted"
        => Session(SessionEvent::WriteFormatted) sample samples::write_formatted;
    RunToolFailed(RunToolFailedPayload) = "run_tool_failed"
        => Session(SessionEvent::RunToolFailed) sample samples::run_tool_failed;
    RunToolRefused(RunToolRefusedPayload) = "run_tool_refused"
        => Session(SessionEvent::RunToolRefused) sample samples::run_tool_refused;
    ServiceUnreachable(ServiceUnreachablePayload) = "service_unreachable"
        => Session(SessionEvent::ServiceUnreachable) sample samples::service_unreachable;
    ServiceReachable(ServiceReachablePayload) = "service_reachable"
        => Session(SessionEvent::ServiceReachable) sample samples::service_reachable;
    HostSuspended(HostSuspendedPayload) = "host_suspended"
        => Run(RunEvent::HostSuspended) sample samples::host_suspended;
    RunPaused(RunPausedPayload) = "run_paused" => Run(RunEvent::Paused) sample samples::run_paused;
    RunResumed(RunResumedPayload) = "run_resumed"
        => Run(RunEvent::Resumed) sample samples::run_resumed;
    RunMoved(RunMovedPayload) = "run_moved" => Run(RunEvent::Moved) sample samples::run_moved;
    RunFinished(RunFinishedPayload) = "run_finished"
        => Run(RunEvent::Finished) sample samples::run_finished;
}
