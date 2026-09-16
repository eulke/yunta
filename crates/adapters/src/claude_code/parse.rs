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

use std::path::Path;

use serde_json::Value;
use yunta_core::{InvalidId, ModelName, SessionId};

use crate::failure;
use yunta_core::events::ToolTarget;
use yunta_core::fence::Coverage;
use yunta_core::port::{AgentError, AgentEvent, AgentOutcome, RunToolsEndpoint};

/// One line of the CLI's stream-json, by the `type` it declares.
///
/// Tagged rather than matched on a string, so the set this adapter reads
/// is a type and a line outside it has a name. `Unknown` is that name:
/// a CLI that adds a line kind — or one this adapter was never taught —
/// is tolerated and says so, never mistaken for something it is not and
/// never a silent nothing.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClaudeLine {
    System(SystemLine),
    Assistant(Value),
    /// A `user` line: what the CLI feeds back into the conversation,
    /// including a tool result. A hook's refusal reaches the stream
    /// here and nowhere else.
    User(Value),
    Result(Value),
    /// A line kind this adapter does not read: a kind the CLI adds
    /// after this build, or a CLI speaking a different protocol.
    #[serde(other)]
    Unknown,
}

/// A `system` line. Only its `init` subtype opens a session; the others
/// report nothing this adapter acts on.
#[derive(Debug, serde::Deserialize)]
struct SystemLine {
    #[serde(default)]
    subtype: Option<String>,
    #[serde(flatten)]
    rest: Value,
}

pub(super) fn parse_line(line: &str, cwd: &Path, fence: Option<&Coverage>) -> Vec<AgentEvent> {
    // A line that is not JSON at all is not this protocol: the stream
    // carries whatever the CLI wrote to stdout, warnings included.
    let Ok(parsed) = serde_json::from_str::<ClaudeLine>(line) else {
        return Vec::new();
    };
    match parsed {
        ClaudeLine::System(system) if system.subtype.as_deref() == Some("init") => {
            opened(&system.rest, fence)
        }
        ClaudeLine::Assistant(value) => assistant_message(&value),
        ClaudeLine::User(value) => refused_writes(&value, cwd),
        ClaudeLine::Result(value) => result_events(&value),
        ClaudeLine::System(_) | ClaudeLine::Unknown => Vec::new(),
    }
}

/// Everything the init line reports: the session it opens, and — when
/// the CLI also names the set of tools that session holds — how many of
/// the run tools are in it. A line that does not open a session reports
/// nothing else: there is no session for a tool to belong to.
fn opened(value: &Value, fence: Option<&Coverage>) -> Vec<AgentEvent> {
    let opened = session_opened(value, fence);
    if matches!(opened, AgentEvent::Failed { .. }) {
        return vec![opened];
    }
    match run_tools_mounted(value) {
        Some(count) => vec![opened, AgentEvent::RunToolsMounted { count }],
        None => vec![opened],
    }
}

/// Every write the fence refused, as the CLI hands the hook's own
/// refusal back: a `tool_result` marked an error whose text the one
/// parser of the marker recognises. Anything else in a `user` line is
/// the conversation, which this adapter does not record.
fn refused_writes(value: &Value, cwd: &Path) -> Vec<AgentEvent> {
    let Some(content) = value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    content
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("tool_result"))
        .filter(|part| part.get("is_error").and_then(Value::as_bool) == Some(true))
        .filter_map(|part| refusal_target(part.get("content")?, cwd))
        .map(|target| AgentEvent::WriteRefused { target })
        .collect()
}

/// The path a refusal names, relative to the work when it is under it:
/// the log records what changed in the worktree, not where the worktree
/// happens to sit on this host. A `content` the CLI wrote as a list of
/// parts is read part by part.
fn refusal_target(content: &Value, cwd: &Path) -> Option<ToolTarget> {
    let text = match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    let target = yunta_core::fence::refused_target(&text)?;
    let shown = target.strip_prefix(cwd).unwrap_or(&target);
    Some(ToolTarget::of_path(shown))
}

/// How many of the per-run server's tools the init line's own tool set
/// names. `None` when the line names no tool set at all: what this CLI
/// does not report is unknown, never zero, and a reader must be able to
/// tell the two apart before it acts on a count.
fn run_tools_mounted(value: &Value) -> Option<usize> {
    let prefix = format!("mcp__{}__", RunToolsEndpoint::SERVER_NAME);
    Some(
        value
            .get("tools")?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .filter(|name| name.starts_with(&prefix))
            .count(),
    )
}

/// The init line names the session, and the model when the CLI says
/// which. A session id that is missing or cannot be one fails the
/// session outright — nothing can resume a session under a name it
/// never had, and the next attempt would read the same line.
fn session_opened(value: &Value, fence: Option<&Coverage>) -> AgentEvent {
    let Some(session_id) = value.get("session_id").and_then(Value::as_str) else {
        return malformed("init line has no `session_id`");
    };
    let session_id = match session_id.parse::<SessionId>() {
        Ok(session_id) => session_id,
        Err(error) => return unreadable("init line's `session_id`", error),
    };
    let model = match value.get("model").and_then(Value::as_str) {
        None => None,
        Some(model) => match model.parse::<ModelName>() {
            Ok(model) => Some(model),
            Err(error) => return unreadable("init line's `model`", error),
        },
    };
    AgentEvent::SessionOpened {
        session_id,
        model,
        fence: fence.cloned(),
    }
}

/// A line the protocol does not allow: the CLI is not speaking
/// stream-json as this adapter reads it, and a retry would read it
/// again.
fn malformed(what: &str) -> AgentEvent {
    failed(AgentError::message(format!("the CLI's {what}")))
}

/// The same, for a field that is present and cannot be what it claims:
/// the failure keeps what rejected it, so a reader following the chain
/// reaches the rule the value broke.
fn unreadable(what: &str, cause: InvalidId) -> AgentEvent {
    failed(AgentError::caused_by(format!("the CLI's {what}"), cause))
}

fn failed(error: AgentError) -> AgentEvent {
    AgentEvent::Failed {
        error,
        retryable: false,
    }
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
            target: tool_target(item.get("input").unwrap_or(&Value::Null)),
        }),
        // "thinking"/"redacted_thinking" and anything future: not
        // surfaced as a Note — internal reasoning, not an operator-facing
        // status update.
        _ => None,
    }
}

/// What a tool call acted on, as this CLI reports it.
///
/// A path names the repository, so the log carries it as written: that
/// is how a reader finds the file. A command, a pattern or a URL is
/// whatever the session typed — the engine cannot vouch for it and never
/// repeats it, so those are identified and not shown.
fn tool_target(input: &Value) -> ToolTarget {
    for key in ["file_path", "path"] {
        if let Some(path) = input.get(key).and_then(Value::as_str) {
            return ToolTarget::of_path(std::path::Path::new(path));
        }
    }
    for key in ["command", "pattern", "url"] {
        if let Some(text) = input.get(key).and_then(Value::as_str) {
            return ToolTarget::opaque(text.as_bytes());
        }
    }
    ToolTarget::opaque(input.to_string().as_bytes())
}

fn result_events(value: &Value) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    if let Some(usage) = value.get("usage") {
        events.push(AgentEvent::Usage {
            input_tokens: usage.get("input_tokens").and_then(Value::as_u64),
            output_tokens: usage.get("output_tokens").and_then(Value::as_u64),
            cached_input_tokens: usage.get("cache_read_input_tokens").and_then(Value::as_u64),
        });
    }

    let Some(is_error) = value.get("is_error").and_then(Value::as_bool) else {
        events.push(malformed("result line has no `is_error`"));
        return events;
    };
    events.push(if is_error {
        let message = value
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or("session ended with an error and no result text")
            .to_string();
        let retryable = failure::classify(&message).retryable();
        AgentEvent::Failed {
            error: AgentError::message(message),
            retryable,
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
