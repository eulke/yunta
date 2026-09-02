//! Parses `claude --output-format stream-json` lines into `AgentEvent`s.
//! Pure and total: a line whose shape we don't recognize — a future
//! stream kind, debug noise from a nested session — yields no events
//! rather than an error, the same tolerant-reader stance the event log
//! itself takes for unknown event kinds.
//!
//! Token accounting trusts only the terminal `result` line's `usage`,
//! never a per-message `assistant` line's: observed CLI output shows an
//! `assistant` message's `output_tokens` undercounting thinking tokens
//! later reconciled in the terminal total. Reporting a single, correct
//! total costs mid-run budget granularity — this adapter can't cut a
//! turn off mid-generation — but never double-counts or under-reports,
//! and correct cost attribution matters more than a cutoff this CLI's
//! atomic-turn execution model can't reliably support anyway.

use serde_json::Value;
use yunta_core::{sha256_hex, ModelName, SessionId};

use crate::session::{AgentError, AgentEvent, AgentOutcome};

pub(super) fn parse_line(line: &str) -> Vec<AgentEvent> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    match value.get("type").and_then(Value::as_str) {
        Some("system") if is_init(&value) => session_opened(&value).into_iter().collect(),
        Some("assistant") => assistant_message(&value),
        Some("result") => result_events(&value),
        _ => Vec::new(),
    }
}

fn is_init(value: &Value) -> bool {
    value.get("subtype").and_then(Value::as_str) == Some("init")
}

/// The init line names the session and the model; a value that cannot
/// be one fails the session explicitly instead of opening it under a
/// name nothing can resume.
fn session_opened(value: &Value) -> Option<AgentEvent> {
    let session_id = value.get("session_id")?.as_str()?;
    let model = value.get("model")?.as_str()?;
    Some(
        match (session_id.parse::<SessionId>(), model.parse::<ModelName>()) {
            (Ok(session_id), Ok(model)) => AgentEvent::SessionOpened { session_id, model },
            (Err(error), _) | (_, Err(error)) => AgentEvent::Failed {
                error: AgentError {
                    message: format!("the CLI's init line is malformed: {error}"),
                },
                retryable: false,
            },
        },
    )
}

fn assistant_message(value: &Value) -> Vec<AgentEvent> {
    let Some(content) = value
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    content.iter().filter_map(content_event).collect()
}

fn content_event(item: &Value) -> Option<AgentEvent> {
    match item.get("type").and_then(Value::as_str)? {
        "text" => Some(AgentEvent::Note {
            text: item.get("text")?.as_str()?.to_string(),
        }),
        "tool_use" => Some(AgentEvent::ToolUse {
            name: item.get("name")?.as_str()?.to_string(),
            target_digest: tool_target_digest(item.get("input").unwrap_or(&Value::Null)),
        }),
        // "thinking"/"redacted_thinking" and anything future: not
        // surfaced as a Note — internal reasoning, not an operator-facing
        // status update.
        _ => None,
    }
}

fn tool_target_digest(input: &Value) -> String {
    for key in ["file_path", "path", "command", "pattern", "url"] {
        if let Some(s) = input.get(key).and_then(Value::as_str) {
            return s.to_string();
        }
    }
    sha256_hex(input.to_string().as_bytes())
}

fn result_events(value: &Value) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    if let Some(usage) = value.get("usage") {
        events.push(AgentEvent::Usage {
            input_tokens: field_u64(usage, "input_tokens"),
            output_tokens: field_u64(usage, "output_tokens"),
            cached_input_tokens: usage.get("cache_read_input_tokens").and_then(Value::as_u64),
        });
    }

    let is_error = value
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    events.push(if is_error {
        AgentEvent::Failed {
            error: AgentError {
                message: value
                    .get("result")
                    .and_then(Value::as_str)
                    .unwrap_or("session ended with an error and no result text")
                    .to_string(),
            },
            // [inferido]: the CLI's `result` subtypes beyond "success"
            // aren't documented field-by-field here; retrying is the
            // safe default — yunta's own max_retries still caps the
            // cost of guessing wrong.
            retryable: true,
        }
    } else {
        AgentEvent::Completed {
            result: AgentOutcome {
                summary: value
                    .get("result")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            },
        }
    });
    events
}

fn field_u64(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}
