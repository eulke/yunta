//! What a node is doing right now, read off the node ledger.
//!
//! Each of these used to walk the log with a rule of its own — where an
//! attempt began, when the node last said anything, which sessions count
//! as open, what the open attempt has spent. They are all facts the
//! [`NodeLedger`](yunta_core::events::NodeLedger) already folds, and
//! this module is what names them in the words a surface asks in.
//!
//! Reads the node domain only.

use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_core::events::{StoredEvent, TokenUsage};
use yunta_core::NodeId;

use crate::replay::{derive, RunState};

pub use yunta_core::events::{OpenSession, ToolCall};

/// When the attempt a node is running now began: its last `node_started`
/// with no terminal after it. `None` for a node that never started and
/// for one whose last attempt already closed — in both cases there is no
/// attempt to time.
pub fn running_since(state: &RunState, node: &NodeId) -> Option<DateTime<Utc>> {
    state
        .nodes
        .get(node)
        .and_then(|record| record.open_since)
        .map(|(_, at)| at)
}

/// How long ago, at `now`, this node wrote its most recent event.
///
/// This is what a view shows instead of a spinner: it is measured, it
/// grows while the node says nothing, and it never claims progress the
/// log does not carry. `None` for a node with no events at all;
/// [`Duration::ZERO`] for an event stamped after `now`, since a node
/// cannot have been silent for a negative time.
pub fn last_event_age(state: &RunState, node: &NodeId, now: DateTime<Utc>) -> Option<Duration> {
    let last = state.nodes.get(node)?.last_event_at?;
    Some((now - last).to_std().unwrap_or(Duration::ZERO))
}

/// The sessions a node has open: every `agent_session_opened` since its
/// last terminal, oldest first.
///
/// A node's attempt is what bounds a session's life — the schema has no
/// per-session close event, so a session counts as open exactly while
/// the attempt that opened it has not reached a terminal. A node running
/// several sessions at once (a `loop` node's tasks) reports all of them.
pub fn open_sessions(state: &RunState, node: &NodeId) -> Vec<OpenSession> {
    state
        .nodes
        .get(node)
        .map(|record| record.sessions.clone())
        .unwrap_or_default()
}

/// The last `limit` tool calls a node made, newest first: its
/// `agent_message` events of type `tool_use` since its last terminal.
///
/// These are the node's calls, not any one session's: the event envelope
/// carries no session id, so a node running several sessions at once
/// reports one stream.
pub fn recent_tool_calls(state: &RunState, node: &NodeId, limit: usize) -> Vec<ToolCall> {
    let Some(record) = state.nodes.get(node) else {
        return Vec::new();
    };
    record.calls.iter().rev().take(limit).cloned().collect()
}

/// What the run has spent including the attempts still open — the number
/// a surface shows while a run is moving.
pub fn live_total_tokens(events: &[StoredEvent]) -> TokenUsage {
    live_total_tokens_of(&derive(events))
}

/// [`live_total_tokens`] for a caller that has already replayed the log
/// — a frame derives once and reads everything off that one pass.
pub(crate) fn live_total_tokens_of(state: &RunState) -> TokenUsage {
    state.total_tokens() + in_flight_tokens(state)
}

/// The usage reported by nodes whose current attempt has not closed:
/// every start opens a node's accounting again from zero, so this is
/// what the attempts now running have reported, never a sum across
/// attempts.
///
/// A closed attempt is paid for by its terminal — or, for a node that
/// asked, by the `questions_asked` that closed its accounting — which is
/// why this drops the moment one arrives.
fn in_flight_tokens(state: &RunState) -> TokenUsage {
    state
        .nodes
        .values()
        .filter(|record| record.open_since.is_some())
        .map(|record| record.tokens_in_flight)
        .sum()
}
