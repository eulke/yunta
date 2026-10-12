//! Parses `claude --output-format stream-json` lines into `AgentEvent`s.
//! Pure and total: a line whose shape we don't recognize — a future
//! stream kind, debug noise from a nested session — yields no events
//! rather than an error, the same tolerant-reader stance the event log
//! itself takes for unknown event kinds.
//!
//! Tokens are reported as the session spends them, in the shape every
//! adapter reports them: what a message read counts as its input, cache
//! reads and writes included, and the cache reads are the cached part of
//! it. Each assistant message's `usage` is reported once — the CLI repeats
//! it on every line that carries one of the message's blocks — and the
//! terminal `result` line reports whatever its totals hold beyond what the
//! messages said: a message's `output_tokens` undercounts what the
//! `result` reconciles, while the input it read adds up to the total
//! exactly.

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use yunta_core::{InvalidId, ModelName, RunTool, SessionId, ToolNaming};

use crate::failure;
use yunta_core::events::ToolTarget;
use yunta_core::fence::Coverage;
use yunta_core::port::{AgentError, AgentEvent, AgentOutcome};

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

pub(super) fn parse_line(
    line: &str,
    fence: Option<&Coverage>,
    run_tool_calls: &mut HashMap<String, RunTool>,
    reported: &mut Reported,
) -> Vec<AgentEvent> {
    // A line that is not JSON at all is not this protocol: the stream
    // carries whatever the CLI wrote to stdout, warnings included.
    let Ok(parsed) = serde_json::from_str::<ClaudeLine>(line) else {
        return Vec::new();
    };
    match parsed {
        ClaudeLine::System(system) if system.subtype.as_deref() == Some("init") => {
            vec![session_opened(&system.rest, fence)]
        }
        ClaudeLine::Assistant(value) => {
            let mut events = assistant_message(&value, run_tool_calls);
            events.extend(message_usage(&value, reported));
            events
        }
        ClaudeLine::User(value) => failed_run_tools(&value, run_tool_calls),
        ClaudeLine::Result(value) => result_events(&value, reported),
        ClaudeLine::System(_) | ClaudeLine::Unknown => Vec::new(),
    }
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
        cause: yunta_core::port::FailureCause::Final,
    }
}

fn assistant_message(
    value: &Value,
    run_tool_calls: &mut HashMap<String, RunTool>,
) -> Vec<AgentEvent> {
    let Some(content) = value
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    for item in content {
        if item.get("type").and_then(Value::as_str) == Some("tool_use") {
            if let (Some(id), Some(name)) = (
                item.get("id").and_then(Value::as_str),
                item.get("name").and_then(Value::as_str),
            ) {
                if let Some(tool) = RunTool::from_called(name, ToolNaming::McpPrefixed) {
                    run_tool_calls.insert(id.to_string(), tool);
                }
            }
        }
    }
    content.iter().filter_map(content_event).collect()
}

fn failed_run_tools(
    value: &Value,
    run_tool_calls: &mut HashMap<String, RunTool>,
) -> Vec<AgentEvent> {
    let Some(content) = value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    content
        .iter()
        .filter_map(|part| {
            if part.get("type").and_then(Value::as_str) != Some("tool_result") {
                return None;
            }
            let id = part.get("tool_use_id")?.as_str()?;
            let tool = run_tool_calls.remove(id)?;
            if part.get("is_error").and_then(Value::as_bool) == Some(true) {
                Some(AgentEvent::RunToolFailed {
                    tool,
                    cause: yunta_core::events::RunToolFailureCause::CallFailed,
                })
            } else {
                None
            }
        })
        .collect()
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

/// What this session's usage has reported so far: every assistant message
/// once, by its id, and the totals they added up to.
#[derive(Debug, Default)]
pub(super) struct Reported {
    messages: HashSet<String>,
    spent: Spent,
}

/// Tokens in the shape every adapter reports: the input a call read,
/// cache included, the part of it read from the cache, and the output. A
/// count the CLI did not give stays absent: absent is not zero.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Spent {
    input: Option<u64>,
    cached: Option<u64>,
    output: Option<u64>,
}

impl Spent {
    /// A `usage` object of this CLI's in that shape.
    fn of(usage: &Value) -> Self {
        let count = |field: &str| usage.get(field).and_then(Value::as_u64);
        let read = [
            "input_tokens",
            "cache_read_input_tokens",
            "cache_creation_input_tokens",
        ]
        .map(count);
        Spent {
            input: read
                .iter()
                .any(Option::is_some)
                .then(|| read.iter().flatten().sum()),
            cached: count("cache_read_input_tokens"),
            output: count("output_tokens"),
        }
    }

    /// Both counts together, field by field.
    fn plus(self, other: Spent) -> Spent {
        let add = |a: Option<u64>, b: Option<u64>| match (a, b) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        };
        Spent {
            input: add(self.input, other.input),
            cached: add(self.cached, other.cached),
            output: add(self.output, other.output),
        }
    }

    /// What `self` holds beyond `said`, never below nothing.
    fn beyond(self, said: Spent) -> Spent {
        let rest = |total: Option<u64>, said: Option<u64>| {
            total.map(|t| t.saturating_sub(said.unwrap_or(0)))
        };
        Spent {
            input: rest(self.input, said.input),
            cached: rest(self.cached, said.cached),
            output: rest(self.output, said.output),
        }
    }

    /// Whether this says a count above nothing.
    fn says_anything(self) -> bool {
        [self.input, self.cached, self.output]
            .into_iter()
            .flatten()
            .any(|count| count > 0)
    }

    fn as_usage(self) -> AgentEvent {
        AgentEvent::Usage {
            input_tokens: self.input,
            output_tokens: self.output,
            cached_input_tokens: self.cached,
        }
    }
}

/// An assistant message's usage, the first time one of its lines carries
/// it.
fn message_usage(value: &Value, reported: &mut Reported) -> Option<AgentEvent> {
    let message = value.get("message")?;
    let id = message.get("id")?.as_str()?;
    let usage = message.get("usage")?;
    if !reported.messages.insert(id.to_string()) {
        return None;
    }
    let spent = Spent::of(usage);
    reported.spent = reported.spent.plus(spent);
    Some(spent.as_usage())
}

fn result_events(value: &Value, reported: &mut Reported) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    if let Some(usage) = value.get("usage") {
        let total = Spent::of(usage);
        let rest = total.beyond(reported.spent);
        reported.spent = total;
        if rest.says_anything() {
            events.push(rest.as_usage());
        }
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
        let cause = failure::classify(&message).cause();
        AgentEvent::Failed {
            error: AgentError::message(message),
            cause,
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
