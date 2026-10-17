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
use yunta_core::{RunTool, SessionId};

use crate::failure::{classify, has_word};
use yunta_core::events::{FailureCause, RefusalCause, ToolTarget};
use yunta_core::fence::Coverage;
use yunta_core::port::{AgentError, AgentEvent, AgentOutcome};

/// One `codex exec --json` event, by the `type` it declares — the same
/// closed set `ThreadEvent` defines, as this module's own doc
/// transcribes it.
///
/// Tagged rather than matched on a string, so a line outside the set has
/// a name. `Unknown` is that name: `turn.started`, `item.started` and
/// `item.updated` are kinds this adapter deliberately does not act on,
/// and a kind the CLI adds later lands here too — tolerated and named,
/// never mistaken for something else.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type")]
enum ThreadEvent {
    #[serde(rename = "thread.started")]
    ThreadStarted(Value),
    #[serde(rename = "item.completed")]
    ItemCompleted(Value),
    #[serde(rename = "turn.completed")]
    TurnCompleted(Value),
    #[serde(rename = "turn.failed")]
    TurnFailed { error: Option<Value> },
    #[serde(rename = "error")]
    Error(Value),
    #[serde(other)]
    Unknown,
}

pub(super) fn parse_line(line: &str, last_message: &str, fence: &Coverage) -> Vec<AgentEvent> {
    // A line that is not JSON at all is not this protocol: the stream
    // carries whatever the CLI wrote to stdout, warnings included.
    let Ok(parsed) = serde_json::from_str::<ThreadEvent>(line) else {
        return Vec::new();
    };
    match parsed {
        ThreadEvent::ThreadStarted(value) => thread_started(&value, fence).into_iter().collect(),
        ThreadEvent::ItemCompleted(value) => item_completed(&value),
        ThreadEvent::TurnCompleted(value) => turn_completed(&value, last_message),
        ThreadEvent::TurnFailed { error } => vec![failed(error.as_ref(), "turn failed")],
        ThreadEvent::Error(value) => vec![failed(Some(&value), "the CLI reported an error")],
        ThreadEvent::Unknown => Vec::new(),
    }
}

/// `thread.started` names the session; a thread id that cannot be one
/// fails the session explicitly instead of opening it under a name
/// nothing can resume.
fn thread_started(value: &Value, fence: &Coverage) -> Option<AgentEvent> {
    let thread_id = value.get("thread_id")?.as_str()?;
    Some(match thread_id.parse::<SessionId>() {
        Ok(session_id) => AgentEvent::SessionOpened {
            session_id,
            model: None,
            fence: Some(fence.clone()),
        },
        Err(error) => AgentEvent::Failed {
            // The failure keeps what rejected the id, so a reader
            // following the chain reaches the rule the value broke.
            error: AgentError::caused_by("the CLI's `thread.started` line is malformed", error),
            cause: FailureCause::Refused(RefusalCause::Final),
        },
    })
}

fn item_completed(value: &Value) -> Vec<AgentEvent> {
    let Some(item) = value.get("item") else {
        return Vec::new();
    };
    let event = match item.get("type").and_then(Value::as_str) {
        Some("agent_message") => {
            item.get("text")
                .and_then(Value::as_str)
                .map(|text| AgentEvent::Note {
                    text: text.to_string(),
                })
        }
        Some("command_execution") => Some(AgentEvent::ToolUse {
            name: "command_execution".to_string(),
            target: opaque_field(item, "command"),
        }),
        Some("file_change") => Some(AgentEvent::ToolUse {
            name: "file_change".to_string(),
            target: file_change_target(item),
        }),
        Some("mcp_tool_call") => Some(AgentEvent::ToolUse {
            name: mcp_tool_name(item),
            target: mcp_tool_call_target(item),
        }),
        Some("web_search") => Some(AgentEvent::ToolUse {
            name: "web_search".to_string(),
            target: opaque_field(item, "query"),
        }),
        // "reasoning", "todo_list", "error" (mid-turn, non-fatal): not
        // operator-facing tool activity — see this module's own doc.
        _ => None,
    };
    let mut events: Vec<_> = event.into_iter().collect();
    if let Some(rejected) = run_tool_rejection(item) {
        events.push(rejected);
    }
    events
}

/// A run tool call of this run's server that the CLI reports as not done.
/// Whether the call reached the engine is the engine's to read, so the
/// status says only that the call was refused.
fn run_tool_rejection(item: &Value) -> Option<AgentEvent> {
    if item.get("type").and_then(Value::as_str) != Some("mcp_tool_call")
        || item.get("server").and_then(Value::as_str)
            != Some(yunta_core::port::RunToolsEndpoint::SERVER_NAME)
    {
        return None;
    }
    let tool = RunTool::parse(item.get("tool")?.as_str()?)?;
    let status = item.get("status")?.as_str()?;
    matches!(
        status,
        "failed" | "approval_denied" | "approval_blocked" | "declined"
    )
    .then_some(AgentEvent::RunToolRejected { tool })
}

/// The field this kind of item acts on, identified and never shown: it
/// is the session's own text, which the engine cannot vouch for. The
/// whole item stands in when the field is absent.
fn opaque_field(item: &Value, key: &str) -> ToolTarget {
    match item.get(key).and_then(Value::as_str) {
        Some(found) => ToolTarget::opaque(found.as_bytes()),
        None => ToolTarget::opaque(item.to_string().as_bytes()),
    }
}

/// `changes` is a list (a single `file_change` item can touch several
/// paths at once) — the first path represents the call, the same "pick
/// one meaningful field" convention `claude_code::parse` uses. A path
/// names the repository, so it is shown.
fn file_change_target(item: &Value) -> ToolTarget {
    item.get("changes")
        .and_then(Value::as_array)
        .and_then(|changes| changes.first())
        .and_then(|change| change.get("path"))
        .and_then(Value::as_str)
        .map(|path| ToolTarget::of_path(std::path::Path::new(path)))
        .unwrap_or_else(|| ToolTarget::opaque(item.to_string().as_bytes()))
}

/// The name of an MCP call. A run tool of this run's server is named as the
/// run tool it is, so the log reads one name for it under every CLI; another
/// server's tool keeps the generic name.
fn mcp_tool_name(item: &Value) -> String {
    let ours = item.get("server").and_then(Value::as_str)
        == Some(yunta_core::port::RunToolsEndpoint::SERVER_NAME);
    let run_tool = ours
        .then(|| {
            item.get("tool")
                .and_then(Value::as_str)
                .and_then(RunTool::parse)
        })
        .flatten();
    run_tool.map_or_else(
        || "mcp_tool_call".to_string(),
        |tool| tool.name().to_string(),
    )
}

/// Which server and which tool — names the workflow itself declares, so
/// a reader may see them.
fn mcp_tool_call_target(item: &Value) -> ToolTarget {
    match (
        item.get("server").and_then(Value::as_str),
        item.get("tool").and_then(Value::as_str),
    ) {
        (Some(server), Some(tool)) => ToolTarget::named(&format!("{server}:{tool}")),
        _ => ToolTarget::opaque(item.to_string().as_bytes()),
    }
}

fn turn_completed(value: &Value, last_message: &str) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    if let Some(usage) = value.get("usage") {
        events.push(AgentEvent::Usage {
            input_tokens: usage.get("input_tokens").and_then(Value::as_u64),
            output_tokens: usage.get("output_tokens").and_then(Value::as_u64),
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

/// Codex's limit phrases. Only this parser reads them: the shared reading
/// has no limit class, so another CLI's text never reads as one.
const LIMIT_MARKERS: &[&str] = &[
    "you\u{2019}ve hit your usage limit",
    "you've hit your usage limit",
    "exceeded retry limit, last status: 429",
];

/// The cause of a Codex failure text. Codex retries a request on its own
/// before it fails with a limit, so a limit it names is the account's, and
/// the text names no reset time.
pub(crate) fn failure_cause(message: &str) -> FailureCause {
    let lower = message.to_lowercase();
    if LIMIT_MARKERS.iter().any(|marker| has_word(&lower, marker)) {
        FailureCause::Limit { resets_at: None }
    } else {
        classify(message)
    }
}

/// `turn.failed` and the top-level `error` both carry a `message`, and the
/// message is what refused the turn.
fn failed(carrier: Option<&Value>, fallback: &str) -> AgentEvent {
    let message = carrier
        .and_then(|carrier| carrier.get("message"))
        .and_then(Value::as_str)
        .map_or_else(
            || format!("{fallback} with no error message"),
            str::to_string,
        );
    let cause = failure_cause(&message);
    AgentEvent::Failed {
        error: AgentError::message(message),
        cause,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_apostrophes_are_codex_limit_phrases() {
        for message in [
            "You\u{2019}ve hit your usage limit. Try again later.",
            "You've hit your usage limit. Try again later.",
        ] {
            assert_eq!(
                failure_cause(message),
                FailureCause::Limit { resets_at: None },
                "{message}"
            );
        }
    }
}
