//! Parses `codex exec --json` lines into `AgentEvent`s (T7.4). Pure and
//! total, same stance as `claude_code::parse`: an unrecognized line
//! shape yields no events rather than an error.
//!
//! **Protocol shape, confirmed from real `codex exec --json` runs**
//! (openai/codex — a gist of 81 empirically-tested flag/feature
//! invocations, plus corroborating GitHub issues where a field's
//! presence was in question):
//! - `{"type":"thread.started","thread_id":"..."}` — first event always,
//!   no `model` field (open gap in the CLI, openai/codex#14736) — see
//!   this module's own caller for how the request's own model fills in.
//! - `{"type":"turn.started"}` — carries nothing this adapter needs.
//! - `{"type":"item.completed","item":{"id":...,"type":"agent_message","text":...}}`
//!   — the turn's own text; `"type":"command_execution"` items carry
//!   `command`/`aggregated_output`/`exit_code`/`status` instead.
//!   `"reasoning"` items exist but, like claude_code's `thinking`
//!   blocks, are internal reasoning rather than an operator-facing
//!   status update — not surfaced.
//! - `{"type":"turn.completed","usage":{"input_tokens":...,"cached_input_tokens":...,"output_tokens":...}}`
//!   — carries no text of its own; the outcome summary comes from the
//!   last `agent_message` item this same turn produced, tracked by the
//!   caller (`mod.rs`) across lines and passed in here.
//! - `{"type":"turn.failed","error":{"message":"..."}}`.

use serde_json::Value;
use yunta_core::{sha256_hex, SessionId};

use crate::session::{AgentError, AgentEvent, AgentOutcome};

pub(super) fn parse_line(line: &str, requested_model: &str, last_message: &str) -> Vec<AgentEvent> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    match value.get("type").and_then(Value::as_str) {
        Some("thread.started") => thread_started(&value, requested_model)
            .into_iter()
            .collect(),
        Some("item.completed") => item_completed(&value).into_iter().collect(),
        Some("turn.completed") => turn_completed(&value, last_message),
        Some("turn.failed") => vec![turn_failed(&value)],
        // "turn.started" and anything future: nothing this adapter needs.
        _ => Vec::new(),
    }
}

fn thread_started(value: &Value, requested_model: &str) -> Option<AgentEvent> {
    let thread_id = value.get("thread_id")?.as_str()?.to_string();
    Some(AgentEvent::SessionOpened {
        session_id: SessionId::from(thread_id),
        model: requested_model.to_string(),
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
            target_digest: command_digest(item),
        }),
        // "reasoning" and anything future: internal, not operator-facing.
        _ => None,
    }
}

fn command_digest(item: &Value) -> String {
    match item.get("command").and_then(Value::as_str) {
        Some(command) => command.to_string(),
        None => sha256_hex(item.to_string().as_bytes()),
    }
}

fn turn_completed(value: &Value, last_message: &str) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    if let Some(usage) = value.get("usage") {
        events.push(AgentEvent::Usage {
            input_tokens: field_u64(usage, "input_tokens"),
            output_tokens: field_u64(usage, "output_tokens"),
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

fn turn_failed(value: &Value) -> AgentEvent {
    AgentEvent::Failed {
        error: AgentError {
            message: value
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("turn failed with no error message")
                .to_string(),
        },
        // [inferido]: `turn.failed` doesn't document a retryable/fatal
        // distinction anywhere seen — same default claude_code's own
        // parser uses for its own undocumented case, for the same
        // reason: yunta's own max_retries still caps the cost (§5.2).
        retryable: true,
    }
}

fn field_u64(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}
