//! The session side of a node's record: the sessions its attempt has
//! open, the writes their fences refused and the calls they made.

use chrono::{DateTime, Utc};

use super::{NodeLedger, NodeRecord};
use crate::events::meta::EventMeta;
use crate::events::{AgentMessagePayload, AgentMessageType, SubjectError, TokenUsage};

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
            confinement: p.confinement.clone(),
            opened_at: at,
        });
    }

    /// The model the rail heard for an open session that opened without
    /// one. A model the session reported at its opening stays as it is.
    fn heard_model(&mut self, p: &crate::events::RailSummaryPayload) {
        let open = self
            .sessions
            .iter_mut()
            .find(|open| open.session_id == p.session_id);
        if let Some(open) = open.filter(|open| open.model.is_none()) {
            open.model = p.model.clone();
        }
    }

    /// What an agent message of the open attempt's session records: a tool
    /// it called, or the usage it reported. Usage moves the spend only while
    /// an attempt is open; reported outside one, it belongs to no attempt.
    fn message(&mut self, p: &AgentMessagePayload, at: DateTime<Utc>) {
        match p.message_type {
            AgentMessageType::ToolUse => self.calls.push(ToolCall {
                tool_name: p.tool_name.clone(),
                target: p.target.clone(),
                at,
            }),
            AgentMessageType::Usage => {
                if self.open_since.is_some() {
                    self.spend.in_flight += TokenUsage {
                        input: p.input_tokens.unwrap_or_default(),
                        output: p.output_tokens.unwrap_or_default(),
                        cached: p.cached_input_tokens,
                    };
                }
            }
            // What the agent said about itself, for a reader. It
            // moves nothing.
            AgentMessageType::Note => {}
        }
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
    /// What the session was opened to write and reach; `None` on a log
    /// written before sessions recorded it.
    pub confinement: Option<crate::events::ConfinementRecord>,
    pub opened_at: DateTime<Utc>,
}

/// One write the fence refused: which session tried it, and what it
/// would have touched.
#[derive(Debug, Clone, PartialEq)]
pub struct RefusedWrite {
    pub session_id: crate::ids::SessionId,
    pub target: crate::events::ToolTarget,
}

/// One stop sent back to the work: which session asked, and the state of
/// its work it was refused at.
#[derive(Debug, Clone, PartialEq)]
pub struct RefusedStop {
    pub session_id: crate::ids::SessionId,
    pub state: crate::ContentHash,
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
    /// Whether `session` of `node` was already sent back from a stop with
    /// its work at `state`: a stop is refused once for one state, so a
    /// session that cannot do better is never held in place.
    pub fn stop_refused_on(
        &self,
        node: &crate::ids::NodeId,
        session: &crate::ids::SessionId,
        state: &crate::ContentHash,
    ) -> bool {
        self.per_node.get(node).is_some_and(|record| {
            record
                .stops_refused
                .iter()
                .any(|stop| &stop.session_id == session && &stop.state == state)
        })
    }

    /// Folds a session-domain event onto the node that wrote it: a
    /// session belongs to the attempt that opened it, so what is open
    /// and what it has spent is part of that node's record. A session
    /// fact with no node belongs to no attempt, so it is refused.
    pub fn apply_session(
        &mut self,
        event: &crate::events::session::kinds::SessionEvent,
        meta: &EventMeta<'_>,
    ) -> Result<(), SubjectError> {
        use crate::events::session::kinds::SessionEvent;

        let Some(node) = meta.node else {
            return Err(SubjectError::SessionWithoutNode);
        };
        // The id is copied only the first time the log names the node:
        // almost every session fact names a node the ledger holds.
        let record = match self.per_node.get_mut(node) {
            Some(record) => record,
            None => self.per_node.entry(node.clone()).or_default(),
        };
        let before = record.standing();
        record.last_event_at = Some(meta.at);
        match event {
            SessionEvent::Opened(p) => record.opened(p, meta.at),
            SessionEvent::WriteRefused(p) => record.refused.push(RefusedWrite {
                session_id: p.session_id.clone(),
                target: p.target.clone(),
            }),
            SessionEvent::StopRefused(p) => record.stops_refused.push(RefusedStop {
                session_id: p.session_id.clone(),
                state: p.state.clone(),
            }),
            SessionEvent::RunToolFailed(p) => record.last_tool_failure = Some(p.clone()),
            // What was formatted is `stats`' and the chronicle's to read.
            SessionEvent::WriteFormatted(_) => {}
            // A failed rail call is audit only: the log keeps it, and no
            // node's state reads it.
            SessionEvent::RailFailed(_) => {}
            SessionEvent::RailSummary(p) => record.heard_model(p),
            // The refusal itself reaches a reader through the log; for the
            // node it is a moment of activity and nothing more.
            SessionEvent::RunToolRefused(_) => {}
            // A session waiting for its service is still the attempt's;
            // how long it waited is `stats`' to read off the log.
            SessionEvent::ServiceUnreachable(_) => record.unreachable = true,
            SessionEvent::ServiceReachable(_) => record.unreachable = false,
            SessionEvent::Message(p) => record.message(p, meta.at),
            // A degradation is the adapter's, not the attempt's: the
            // session ledger holds it.
            SessionEvent::CapabilityDegraded(_) => {}
        }
        let after = record.standing();
        self.track(before, after);
        Ok(())
    }
}
