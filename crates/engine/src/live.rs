//! What a run looks like *right now*, derived from the log it has
//! written so far and an instant the caller supplies. Every function
//! here is pure: same events, same `now`, same answer. The clock is the
//! caller's — nothing in this module reads one, so a view rendered at a
//! chosen instant is reproducible in a test.
//!
//! These are the facts a log carries that [`crate::replay`]'s node and
//! task states do not: when the attempt now running started, how long a
//! node has been silent, which sessions it has open, what it last
//! reached for, and what the run has spent including the work still in
//! flight. A run's *state* stays [`crate::replay::derive`]'s answer;
//! this is the moving picture around it.
//!
//! **Everything here is node-level**, because the event envelope is: it
//! carries `run_id`, `seq`, `timestamp` and `node_id`, and nothing that
//! names a session or a task. A `loop` node running several task
//! sessions at once therefore reports one stream of tool calls, not one
//! per session — the log cannot say which session made a call, so
//! neither can this. That is a limit of the event schema, named here so
//! no reader mistakes it for an omission.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

use yunta_core::events::{
    AgentMessagePayload, AgentMessageType, EventPayload, StoredEvent, TokenUsage,
};
use yunta_core::{AgentName, ModelName, NodeId, SessionId};

use crate::replay::{derive, RunState};

/// When the attempt a node is running now began: the timestamp of its
/// last `node_started` with no `node_finished`/`node_failed` after it.
/// `None` for a node that never started and for one whose last attempt
/// already closed — in both cases there is no attempt to time.
pub fn running_since(events: &[StoredEvent], node: &NodeId) -> Option<DateTime<Utc>> {
    events
        .iter()
        .rev()
        .filter(|event| event.node_id.as_ref() == Some(node))
        .find_map(|event| match event.payload() {
            Some(EventPayload::NodeStarted(_)) => Some(Some(event.timestamp)),
            Some(EventPayload::NodeFinished(_) | EventPayload::NodeFailed(_)) => Some(None),
            _ => None,
        })
        .flatten()
}

/// How long ago, at `now`, this node wrote its most recent event.
///
/// This is what a view shows instead of a spinner: it is measured, it
/// grows while the node says nothing, and it never claims progress the
/// log does not carry. `None` for a node with no events at all;
/// [`Duration::ZERO`] for an event stamped after `now`, since a node
/// cannot have been silent for a negative time.
pub fn last_event_age(
    events: &[StoredEvent],
    node: &NodeId,
    now: DateTime<Utc>,
) -> Option<Duration> {
    let last = events
        .iter()
        .rev()
        .find(|event| event.node_id.as_ref() == Some(node))?;
    Some((now - last.timestamp).to_std().unwrap_or(Duration::ZERO))
}

/// One session a node has open, as `agent_session_opened` recorded it.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenSession {
    pub session_id: SessionId,
    /// The adapter's own named agent the session runs as; `None` when the
    /// runner named none.
    pub agent: Option<AgentName>,
    /// The model the CLI reported for the session; `None` when it
    /// reported none.
    pub model: Option<ModelName>,
    pub opened_at: DateTime<Utc>,
}

/// The sessions a node has open: every `agent_session_opened` since its
/// last terminal event, oldest first.
///
/// A node's attempt is what bounds a session's life — the schema has no
/// per-session close event, so a session counts as open exactly while
/// the attempt that opened it has not reached `node_finished` or
/// `node_failed`. A node running several sessions at once (a `loop`
/// node's tasks) reports all of them.
pub fn open_sessions(events: &[StoredEvent], node: &NodeId) -> Vec<OpenSession> {
    since_last_terminal(events, node)
        .filter_map(|event| match event.payload() {
            Some(EventPayload::AgentSessionOpened(p)) => Some(OpenSession {
                session_id: p.session_id.clone(),
                agent: p.agent.clone(),
                model: p.model.clone(),
                opened_at: event.timestamp,
            }),
            _ => None,
        })
        .collect()
}

/// One tool call, as `agent_message` recorded it.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    /// The tool the adapter named; `None` when it reported a call under
    /// no name.
    pub tool_name: Option<String>,
    /// The adapter's digest of what the call touched; `None` when it
    /// reported none.
    pub target_digest: Option<String>,
    pub at: DateTime<Utc>,
}

/// The last `limit` tool calls a node made, newest first: its
/// `agent_message` events of type `tool_use` since its last terminal.
///
/// These are the node's calls, not any one session's, and cannot be
/// otherwise: the event envelope carries `run_id`, `seq`, `timestamp`
/// and `node_id`, so a log cannot attribute a call to one of a `loop`
/// node's concurrent task sessions. A view that lists them under the
/// node shows exactly what the log knows — the limit is the event
/// schema's, not this derivation's.
pub fn recent_tool_calls(events: &[StoredEvent], node: &NodeId, limit: usize) -> Vec<ToolCall> {
    let mut calls: Vec<ToolCall> = since_last_terminal(events, node)
        .filter_map(|event| match event.payload() {
            Some(EventPayload::AgentMessage(p)) if p.message_type == AgentMessageType::ToolUse => {
                Some(ToolCall {
                    tool_name: p.tool_name.clone(),
                    target_digest: p.target_digest.clone(),
                    at: event.timestamp,
                })
            }
            _ => None,
        })
        .collect();
    calls.reverse();
    calls.truncate(limit);
    calls
}

/// This node's events after its last terminal, in log order — the window
/// that describes the attempt it is running now (its whole log, for a
/// node that has never closed one).
fn since_last_terminal<'a>(
    events: &'a [StoredEvent],
    node: &'a NodeId,
) -> impl Iterator<Item = &'a StoredEvent> {
    let closed = events.iter().rposition(|event| {
        event.node_id.as_ref() == Some(node)
            && matches!(
                event.payload(),
                Some(EventPayload::NodeFinished(_) | EventPayload::NodeFailed(_))
            )
    });
    events
        .iter()
        .skip(closed.map_or(0, |index| index + 1))
        .filter(move |event| event.node_id.as_ref() == Some(node))
}

/// What the run has spent so far, the work still in flight included:
/// [`derive()`]'s own total (every terminated node's `tokens_used`, plus
/// each finished child run's) plus the usage the nodes still running
/// have reported along the way.
///
/// Those two sources describe the same tokens twice over, so this counts
/// a node's `agent_message` usage only while that node has no terminal
/// after its last `node_started`. The moment the node closes, its spend
/// comes from `node_finished`/`node_failed` alone: a run's total never
/// jumps when a node ends, and never counts an attempt twice.
///
/// Usage reported under no node is left out. Nothing can retire it — a
/// terminal event belongs to a node — so adding it would inflate the
/// run's total for the rest of its life.
pub fn live_total_tokens(events: &[StoredEvent]) -> TokenUsage {
    live_total_tokens_of(&derive(events), events)
}

/// [`live_total_tokens`] for a caller that has already replayed the
/// log — a frame derives once and reads everything off that one pass.
pub(crate) fn live_total_tokens_of(state: &RunState, events: &[StoredEvent]) -> TokenUsage {
    state.total_tokens + in_flight_tokens(events)
}

/// The usage reported by nodes whose current attempt has not closed,
/// counted from each node's last `node_started` onward: every start
/// opens a node's accounting again from zero, so this is what the
/// attempt now running has reported, never a sum across attempts.
///
/// A closed attempt is paid for by its terminal's `tokens_used`, which
/// is why the count drops here the moment one arrives. An attempt closed
/// by nothing — the orphan a resume restarts, whose `node_started`
/// follows another with no terminal between them — leaves its reports
/// behind with it: no terminal ever claimed them, so no total carries
/// them.
fn in_flight_tokens(events: &[StoredEvent]) -> TokenUsage {
    let mut open: HashMap<&NodeId, TokenUsage> = HashMap::new();
    for event in events {
        let Some(node_id) = &event.node_id else {
            continue;
        };
        match event.payload() {
            Some(EventPayload::NodeStarted(_)) => {
                open.insert(node_id, TokenUsage::default());
            }
            Some(EventPayload::NodeFinished(_) | EventPayload::NodeFailed(_)) => {
                open.remove(node_id);
            }
            Some(EventPayload::AgentMessage(p)) if p.message_type == AgentMessageType::Usage => {
                // Usage a node reports before its own `node_started`
                // has no attempt to belong to: a node's accounting opens
                // at its start, and replay rejects a terminal that had
                // no start of its own.
                if let Some(in_flight) = open.get_mut(node_id) {
                    *in_flight += reported_tokens(p);
                }
            }
            _ => {}
        }
    }
    open.into_values().sum()
}

/// The tokens one `agent_message` reports: a figure the adapter left out
/// counts as zero, and `cached` stays unknown unless it reported one —
/// the same reading `TokenUsage` gives everywhere else.
fn reported_tokens(message: &AgentMessagePayload) -> TokenUsage {
    TokenUsage {
        input: message.input_tokens.unwrap_or(0),
        output: message.output_tokens.unwrap_or(0),
        cached: message.cached_input_tokens,
    }
}
