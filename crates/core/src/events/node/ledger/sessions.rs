//! The session side of a node's record: the sessions its attempt has
//! open, the writes their fences refused and the calls they made.

use chrono::{DateTime, Utc};

use super::{NodeLedger, NodeRecord};
use crate::events::meta::EventMeta;
use crate::events::TokenUsage;

impl NodeRecord {
    /// A session this node's attempt opened: open while the attempt is,
    /// and — when it is the node's own, not a task's — the one a
    /// continuation picks back up.
    fn opened(&mut self, p: &crate::events::AgentSessionOpenedPayload, at: DateTime<Utc>) {
        if p.task_id.is_none() {
            self.last_session = Some(p.session_id.clone());
        }
        self.sessions.push(OpenSession {
            session_id: p.session_id.clone(),
            agent: p.agent.clone(),
            model: p.model.clone(),
            fence: p.fence.clone(),
            opened_at: at,
        });
    }
}

/// What a resume finds of the attempt before this one, when that attempt
/// never reached a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrphanedSession {
    /// The session it had open, by the id its adapter gave it.
    Open(crate::ids::SessionId),
    /// It opened none before it was cut.
    NoneRecorded,
}

/// One session a node has open, as `agent_session_opened` recorded it.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenSession {
    pub session_id: crate::ids::SessionId,
    /// The adapter's own named agent the session runs as; `None` when
    /// the runner named none.
    pub agent: Option<crate::ids::AgentName>,
    /// The model the CLI reported for the session; `None` when it
    /// reported none.
    pub model: Option<crate::ids::ModelName>,
    /// How much of this session the adapter's fence covered; `None`
    /// when it built none.
    pub fence: Option<crate::fence::Coverage>,
    pub opened_at: DateTime<Utc>,
}

/// One write the fence refused: which session tried it, and what it
/// would have touched.
#[derive(Debug, Clone, PartialEq)]
pub struct RefusedWrite {
    pub session_id: crate::ids::SessionId,
    pub target: crate::events::ToolTarget,
}

/// One tool call, as `agent_message` recorded it.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    /// The tool the adapter named; `None` when it reported a call under
    /// no name.
    pub tool_name: Option<String>,
    /// What the call acted on, as the log carries it; `None` when the
    /// adapter reported none.
    pub target: Option<crate::events::ToolTarget>,
    pub at: DateTime<Utc>,
}

impl NodeLedger {
    /// Folds a session-domain event onto the node that wrote it: a
    /// session belongs to the attempt that opened it, so what is open
    /// and what it has spent is part of that node's record.
    pub fn apply_session(
        &mut self,
        event: &crate::events::session::kinds::SessionEvent,
        meta: &EventMeta<'_>,
    ) {
        use crate::events::session::kinds::SessionEvent;
        use crate::events::AgentMessageType;

        let Some(node) = meta.node else {
            return;
        };
        let record = self.per_node.entry(node.clone()).or_default();
        record.last_event_at = Some(meta.at);
        match event {
            SessionEvent::Opened(p) => record.opened(p, meta.at),
            SessionEvent::WriteRefused(p) => record.refused.push(RefusedWrite {
                session_id: p.session_id.clone(),
                target: p.target.clone(),
            }),
            SessionEvent::RunToolFailed(p) => record.last_tool_failure = Some(p.clone()),
            // The refusal itself reaches a reader through the log; for the
            // node it is a moment of activity and nothing more.
            SessionEvent::RunToolRefused(_) => {}
            // A session waiting for its service is still the attempt's;
            // how long it waited is `stats`' to read off the log.
            SessionEvent::ServiceUnreachable(_) | SessionEvent::ServiceReachable(_) => {}
            SessionEvent::Message(p) => match p.message_type {
                AgentMessageType::ToolUse => record.calls.push(ToolCall {
                    tool_name: p.tool_name.clone(),
                    target: p.target.clone(),
                    at: meta.at,
                }),
                AgentMessageType::Usage => {
                    record.tokens_in_flight += TokenUsage {
                        input: p.input_tokens.unwrap_or_default(),
                        output: p.output_tokens.unwrap_or_default(),
                        cached: p.cached_input_tokens,
                    }
                }
                // What the agent said about itself, for a reader. It
                // moves nothing.
                AgentMessageType::Note => {}
            },
            // A degradation is the adapter's, not the attempt's: the
            // session ledger holds it.
            SessionEvent::CapabilityDegraded(_) => {}
        }
    }
}
