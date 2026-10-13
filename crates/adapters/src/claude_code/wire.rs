//! The lines the `claude` CLI prints for a session, written from its own
//! stream format, and what its hooks send on stdin. The mock plays a
//! scripted session through these lines and reads them back through the
//! same reader a real session's output goes through, so what comes out is
//! what the reader makes of the CLI's words, never what the script said.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};
use yunta_core::rail::RailEvent;
use yunta_core::{ModelName, SessionId, ToolNaming};

use crate::mock::{MockOutcome, MockStep};

/// Counts the ids given out, so every message and tool use of the process
/// carries one of its own. A session counts a message's usage once per id,
/// so two messages sharing one would drop the second's count.
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// The lines a session's stream carries for one step. A run tool's call is
/// printed by the player that made it, with the name it was called by, so
/// a run tool step prints nothing here.
pub(crate) fn render(step: &MockStep, _naming: ToolNaming) -> Vec<String> {
    match step {
        MockStep::Note { text, .. } | MockStep::BeforeOpen { text, .. } => {
            vec![assistant(json!([{ "type": "text", "text": text }]), None)]
        }
        // Kinds the stream carries that the reader does not read, as the CLI writes them.
        MockStep::UnknownLine { .. } => vec![
            json!({ "type": "compact_boundary", "reason": "context" }).to_string(),
            json!({ "type": "user", "message": { "content": [] } }).to_string(),
            json!({ "type": "system", "subtype": "status", "note": "still here" }).to_string(),
        ],
        MockStep::ToolUse { name, target, .. } => vec![assistant(
            json!([{
                "type": "tool_use",
                "id": next_id("toolu"),
                "name": name,
                "input": { "command": target },
            }]),
            None,
        )],
        MockStep::Usage {
            input_tokens,
            output_tokens,
            cached_input_tokens,
            ..
        } => vec![assistant(
            json!([]),
            Some(usage_of(
                *input_tokens,
                *output_tokens,
                *cached_input_tokens,
            )),
        )],
        MockStep::RunTool { .. } | MockStep::RunToolRejected { .. } => Vec::new(),
    }
}

/// The line the CLI closes a session with: a `result` that carries the
/// summary, or on a failure `is_error` and the message. A crash and a hang
/// print no line, since no terminal line exists for either.
pub(crate) fn close(outcome: &MockOutcome) -> Vec<String> {
    match outcome {
        MockOutcome::Completed { summary } => vec![json!({
            "type": "result",
            "is_error": false,
            "result": summary,
        })
        .to_string()],
        MockOutcome::Failed { message, .. } => vec![json!({
            "type": "result",
            "is_error": true,
            "result": message,
        })
        .to_string()],
        MockOutcome::Crash | MockOutcome::Hang { .. } => Vec::new(),
    }
}

/// The init line a session opens with: the id the session is known by, and
/// the model the CLI reports when it reports one.
pub(crate) fn open(session_id: &SessionId, model: Option<&ModelName>) -> Vec<String> {
    let init = match model {
        Some(model) => json!({
            "type": "system",
            "subtype": "init",
            "session_id": session_id.as_str(),
            "model": model.as_str(),
        }),
        None => json!({
            "type": "system",
            "subtype": "init",
            "session_id": session_id.as_str(),
        }),
    };
    vec![init.to_string()]
}

/// What the CLI sends its hook on stdin for `event`: the session asking, and
/// the call the hook judges, a write to `path` or the session's stop.
pub(crate) fn hook_stdin(event: RailEvent, session: &SessionId, path: &Path) -> Vec<u8> {
    let call = match event {
        RailEvent::BeforeStop => json!({
            "session_id": session.as_str(),
            "hook_event_name": "Stop",
            "stop_hook_active": false,
        }),
        RailEvent::BeforeWrite | RailEvent::AfterWrite => json!({
            "session_id": session.as_str(),
            "tool_name": "Write",
            "tool_input": { "file_path": path.display().to_string() },
        }),
    };
    call.to_string().into_bytes()
}

/// An assistant message of `content`, carrying the usage it counts when it
/// counts one.
fn assistant(content: Value, counted: Option<Value>) -> String {
    let message = match counted {
        Some(usage) => json!({ "id": next_id("msg"), "content": content, "usage": usage }),
        None => json!({ "id": next_id("msg"), "content": content }),
    };
    json!({ "type": "assistant", "message": message }).to_string()
}

/// The usage the CLI reports for one message. The cache read is part of the
/// input the message read, so the input it names is what is left of that
/// input beyond the read.
fn usage_of(input: u64, output: u64, cached: Option<u64>) -> Value {
    match cached {
        Some(cached) => json!({
            "input_tokens": input.saturating_sub(cached),
            "cache_read_input_tokens": cached,
            "output_tokens": output,
        }),
        None => json!({ "input_tokens": input, "output_tokens": output }),
    }
}

/// The next id of `prefix`, distinct from every id this process has given out.
fn next_id(prefix: &str) -> String {
    format!("{prefix}_{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
}
