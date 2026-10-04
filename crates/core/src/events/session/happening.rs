//! What a session-level event says happened, read as a person reads it.

use crate::events::{AgentMessageType, SessionEvent, ToolTarget};
use crate::fence::Coverage;
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
        policy: String,
    },
    /// A write the fence turned down, and what it would have touched.
    Refused(ToolTarget),
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
                policy: p.policy_applied().to_string(),
            },
            SessionEvent::WriteRefused(p) => Happening::Refused(p.target.clone()),
            SessionEvent::RunToolFailed(p) => Happening::RunToolFailed {
                tool: p.tool,
                cause: p.cause,
            },
            SessionEvent::RunToolRefused(p) => Happening::RunToolRefused {
                tool: p.tool,
                reason: p.reason,
            },
        }
    }
}
