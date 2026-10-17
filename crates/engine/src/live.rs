//! What a node is doing right now, read off the node ledger.
//!
//! Each of these is a fact the [`NodeLedger`](yunta_core::events::NodeLedger)
//! already folds: where an attempt began, when the node last said anything,
//! which sessions are open, what the open attempt calls were. This module
//! names them in the words a surface asks in.
//!
//! Reads the node domain only.

use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_core::NodeId;

use crate::replay::RunState;

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
