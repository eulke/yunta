//! Parses `codex exec --json` lines into `AgentEvent`s. Pure and total,
//! same stance as `claude_code::parse`: an unrecognized line shape
//! yields no events rather than an error.
//!
//! **Protocol shape — confirmed from the CLI's own source, not
//! guessed.** `codex-rs/exec/src/exec_events.rs` (openai/codex, `main`
//! at the time this was written) defines the wire format directly:
//! `ThreadEvent` is `#[serde(tag = "type")]` over `ThreadStartedEvent`
//! (`thread_id: String` — no `model` field; a confirmed, open gap in
//! the CLI, openai/codex#14736, so the session opens with no model
//! rather than the one the request asked for),
//! `TurnStartedEvent` (empty), `TurnCompletedEvent` (`usage: Usage` —
//! `input_tokens`/`cached_input_tokens`/`output_tokens` plus two fields
//! this adapter doesn't need), `TurnFailedEvent` (`error:
//! ThreadErrorEvent { message: String }`), `ItemStartedEvent`/
//! `ItemUpdatedEvent`/`ItemCompletedEvent` (each wraps a `ThreadItem {
//! id: String, #[serde(flatten)] details: ThreadItemDetails }`), and
//! `ThreadErrorEvent` again for the top-level `error` event.
//! `ThreadItemDetails` is `#[serde(tag = "type", rename_all =
//! "snake_case")]` over one variant per item kind — `AgentMessageItem
//! { text }`, `ReasoningItem { text }`, `CommandExecutionItem {
//! command, aggregated_output, exit_code, status }`, `FileChangeItem {
//! changes: Vec<FileUpdateChange { path, kind }>, status }`,
//! `McpToolCallItem { server, tool, arguments, result, error, status }`,
//! `WebSearchItem { id, query, action }`, `TodoListItem { items }`,
//! `ErrorItem { message }` (a non-fatal, mid-turn error — distinct from
//! `TurnFailedEvent`).
//!
//! Every item type that reads as "the agent used a tool" maps to
//! `ToolUse`, mirroring how `claude_code::parse` treats its own single
//! `tool_use` content block as the general case — `command_execution`,
//! `file_change`, `mcp_tool_call` and `web_search` all qualify.
//! `reasoning` doesn't (internal, not operator-facing — same treatment
//! as claude_code's own `thinking` blocks); `todo_list` and the
//! mid-turn `error` item aren't tool calls at all and aren't surfaced
//! either, for lack of a clear precedent either way — narrower than it
//! could be, not wider than what's confirmed.

use serde_json::Value;
use yunta_core::{sha256_hex, SessionId};

use crate::failure;
use crate::session::{AgentError, AgentEvent, AgentOutcome};

pub(super) fn parse_line(line: &str, last_message: &str) -> Vec<AgentEvent> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    match value.get("type").and_then(Value::as_str) {
        Some("thread.started") => thread_started(&value).into_iter().collect(),
        Some("item.completed") => item_completed(&value).into_iter().collect(),
        Some("turn.completed") => turn_completed(&value, last_message),
        Some("turn.failed") => vec![failed(value.get("error"), "turn failed")],
        Some("error") => vec![failed(Some(&value), "the CLI reported an error")],
        // "turn.started", "item.started"/"item.updated" (this adapter
        // only acts once an item is done) and anything future: nothing
        // this adapter needs.
        _ => Vec::new(),
    }
}

/// `thread.started` names the session; a thread id that cannot be one
/// fails the session explicitly instead of opening it under a name
/// nothing can resume.
fn thread_started(value: &Value) -> Option<AgentEvent> {
    let thread_id = value.get("thread_id")?.as_str()?;
    Some(match thread_id.parse::<SessionId>() {
        Ok(session_id) => AgentEvent::SessionOpened {
            session_id,
            model: None,
        },
        Err(error) => AgentEvent::Failed {
            error: AgentError {
                message: format!("the CLI's `thread.started` line is malformed: {error}"),
            },
            retryable: false,
        },
    })
}

fn item_completed(value: &Value) -> Option<AgentEvent> {
    let item = value.get("item")?;
    match item.get("type").and_then(Value::as_str)? {
        "agent_message" => Some(AgentEvent::Note {
            text: item.get("text")?.as_str()?.to_string(),
        }),
        "command_execution" => Some(AgentEvent::ToolUse {
            name: "command_execution".to_string(),
            target_digest: field_or_hash(item, "command"),
        }),
        "file_change" => Some(AgentEvent::ToolUse {
            name: "file_change".to_string(),
            target_digest: file_change_digest(item),
        }),
        "mcp_tool_call" => Some(AgentEvent::ToolUse {
            name: "mcp_tool_call".to_string(),
            target_digest: mcp_tool_call_digest(item),
        }),
        "web_search" => Some(AgentEvent::ToolUse {
            name: "web_search".to_string(),
            target_digest: field_or_hash(item, "query"),
        }),
        // "reasoning", "todo_list", "error" (mid-turn, non-fatal): not
        // operator-facing tool activity — see this module's own doc.
        _ => None,
    }
}

fn field_or_hash(item: &Value, key: &str) -> String {
    match item.get(key).and_then(Value::as_str) {
        Some(s) => s.to_string(),
        None => sha256_hex(item.to_string().as_bytes()).to_string(),
    }
}

/// `changes` is a list (a single `file_change` item can touch several
/// paths at once) — the first path is the representative digest, same
/// "pick one meaningful field" convention `claude_code::parse` uses for
/// its own multi-field tool inputs.
fn file_change_digest(item: &Value) -> String {
    item.get("changes")
        .and_then(Value::as_array)
        .and_then(|changes| changes.first())
        .and_then(|change| change.get("path"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| sha256_hex(item.to_string().as_bytes()).to_string())
}

fn mcp_tool_call_digest(item: &Value) -> String {
    match (
        item.get("server").and_then(Value::as_str),
        item.get("tool").and_then(Value::as_str),
    ) {
        (Some(server), Some(tool)) => format!("{server}:{tool}"),
        _ => sha256_hex(item.to_string().as_bytes()).to_string(),
    }
}

fn turn_completed(value: &Value, last_message: &str) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    if let Some(usage) = value.get("usage") {
        events.push(AgentEvent::Usage {
            input_tokens: field_u64(usage, "input_tokens"),
            output_tokens: field_u64(usage, "output_tokens"),
            // The real `Usage` struct always sends this field (no
            // `#[serde(default)]` on it, unlike `cache_write_input_tokens`)
            // — `Option` here is yunta's own `Usage` type accommodating
            // adapters that never report it at all, not uncertainty
            // about whether codex will.
            cached_input_tokens: usage.get("cached_input_tokens").and_then(Value::as_u64),
        });
    }
    events.push(AgentEvent::Completed {
        result: AgentOutcome {
            summary: last_message.to_string(),
        },
    });
    events
}

/// `turn.failed` and the top-level `error` both carry a `message` and
/// nothing about retrying; the message itself says whether another
/// attempt can do better.
fn failed(carrier: Option<&Value>, fallback: &str) -> AgentEvent {
    let message = carrier
        .and_then(|carrier| carrier.get("message"))
        .and_then(Value::as_str)
        .map_or_else(
            || format!("{fallback} with no error message"),
            str::to_string,
        );
    let retryable = failure::classify(&message).retryable();
    AgentEvent::Failed {
        error: AgentError { message },
        retryable,
    }
}

fn field_u64(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}
