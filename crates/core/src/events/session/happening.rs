//! What a session-level event says happened, read as a person reads it.

use std::collections::BTreeMap;

use crate::events::{
    AgentMessageType, RailFailedPayload, RailSummaryPayload, SessionEvent, ToolTarget,
};
use crate::fence::Coverage;
use crate::port::PolicyApplied;
use crate::rail::hook::RailFailureCause;
use crate::rail::timing::Histogram;
use crate::rail::RailEvent;
use crate::{AdapterId, AgentName, Capability, ModelName};

/// One thing that happened inside a node's session.
#[derive(Debug, Clone, PartialEq)]
pub enum Happening {
    Opened {
        agent: Option<AgentName>,
        model: Option<ModelName>,
        /// How much of the session the adapter's fence covered — what
        /// says whether a write that reached the diff should have been
        /// possible at all. `None` when the adapter built none.
        fence: Option<Coverage>,
    },
    /// A tool the agent called, named apart from every other message
    /// because a call is what a reader scans for.
    Called {
        tool: Option<String>,
        target: Option<ToolTarget>,
    },
    /// Anything else the session said: what kind of message it was,
    /// what the log kept of it — for a note, its size and digest, never
    /// its words — and what it reported spending.
    Message {
        kind: AgentMessageType,
        text: Option<String>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    },
    Degraded {
        capability: Capability,
        adapter: AdapterId,
        /// What the engine did instead. A degradation naming only what
        /// was missing leaves a reader guessing whether anything holds.
        policy: PolicyApplied,
    },
    /// A write the fence turned down, and what it would have touched.
    Refused(ToolTarget),
    /// A stop sent back to the work, and what kept that work from closing.
    StopRefused(Vec<crate::events::StopRefusal>),
    /// What a session wrote, formatted: by which command, what it changed,
    /// how the command exited, and whether it was stopped at its bound.
    Formatted {
        run: crate::ids::CommandName,
        changed: Vec<std::path::PathBuf>,
        exit_code: i32,
        timed_out: bool,
    },
    /// A failure of the rail: the moment it was for, when one could be
    /// read, and its cause.
    RailFailed {
        event: Option<RailEvent>,
        cause: RailFailureCause,
    },
    /// What the rail heard of a session: its model, and how long each
    /// moment's calls took.
    RailSummary {
        model: Option<ModelName>,
        moments: BTreeMap<RailEvent, Histogram>,
    },
    /// One failed call is a moment in the chronicle.
    RunToolFailed {
        tool: crate::RunTool,
        cause: crate::events::RunToolFailureCause,
    },
    /// A call the engine refused, with why.
    RunToolRefused {
        tool: crate::RunTool,
        reason: crate::events::RunToolRefusal,
    },
    /// The session lost its service, saying this.
    CutOff { message: String },
    /// Its service answers again, after this long.
    Reconnected { waited: std::time::Duration },
}

impl From<&SessionEvent> for Happening {
    fn from(event: &SessionEvent) -> Self {
        match event {
            SessionEvent::Opened(p) => Happening::Opened {
                agent: p.agent.clone(),
                model: p.model.clone(),
                fence: p.fence.clone(),
            },
            SessionEvent::Message(p) => match &p.tool_name {
                Some(_) => Happening::Called {
                    tool: p.tool_name.clone(),
                    target: p.target.clone(),
                },
                None => Happening::Message {
                    kind: p.message_type,
                    text: p.text.clone(),
                    input_tokens: p.input_tokens,
                    output_tokens: p.output_tokens,
                },
            },
            SessionEvent::CapabilityDegraded(p) => Happening::Degraded {
                capability: p.capability,
                adapter: p.adapter.clone(),
                policy: p.policy.clone(),
            },
            SessionEvent::WriteRefused(p) => Happening::Refused(p.target.clone()),
            SessionEvent::StopRefused(p) => Happening::StopRefused(p.why.clone()),
            SessionEvent::WriteFormatted(p) => Happening::Formatted {
                run: p.run.clone(),
                changed: p.changed.clone(),
                exit_code: p.exit_code,
                timed_out: p.timed_out,
            },
            SessionEvent::RailFailed(p) => p.into(),
            SessionEvent::RailSummary(p) => p.into(),
            SessionEvent::RunToolFailed(p) => Happening::RunToolFailed {
                tool: p.tool,
                cause: p.cause,
            },
            SessionEvent::RunToolRefused(p) => Happening::RunToolRefused {
                tool: p.tool,
                reason: p.reason,
            },
            SessionEvent::ServiceUnreachable(p) => Happening::CutOff {
                message: p.message.clone(),
            },
            SessionEvent::ServiceReachable(p) => Happening::Reconnected {
                waited: std::time::Duration::from_millis(p.waited_ms),
            },
        }
    }
}

impl From<&RailFailedPayload> for Happening {
    fn from(payload: &RailFailedPayload) -> Self {
        Happening::RailFailed {
            event: payload.event,
            cause: payload.cause,
        }
    }
}

impl From<&RailSummaryPayload> for Happening {
    fn from(payload: &RailSummaryPayload) -> Self {
        Happening::RailSummary {
            model: payload.model.clone(),
            moments: payload.moments.clone(),
        }
    }
}
