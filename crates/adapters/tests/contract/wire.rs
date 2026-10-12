//! The stream lines each CLI prints for a scripted session, written from that
//! CLI's own stream format. No adapter reads these lines back to produce them:
//! the stubs print them to the real adapters, so a parser that misreads its
//! CLI's format fails against lines written from the format itself.

use serde_json::{json, Value};
use yunta_core::port::RunToolsEndpoint;
use yunta_core::RunTool;

use super::subject::{Outcome, Script, Step, STOP_TURN};

/// The session id a Claude stream opens with.
const CLAUDE_SESSION: &str = "contract-session";

/// The thread id a Codex stream opens with.
const CODEX_THREAD: &str = "contract-thread";

/// The usage one Codex turn reports, summed over the script's usage steps as
/// `(input, output, cached)`.
type Spent = (u64, u64, Option<u64>);

/// The stream-json lines a Claude CLI prints for `script`: the lines it
/// prints before its session opens, its init line, one line per step, and the
/// terminal result.
pub fn claude_lines(script: &Script) -> Vec<String> {
    let (before_open, steps) = split_open(&script.steps);
    let mut init = json!({ "type": "system", "subtype": "init", "session_id": CLAUDE_SESSION });
    if let Some(model) = &script.model {
        init["model"] = json!(model);
    }
    let mut lines: Vec<Value> = before_open
        .into_iter()
        .flat_map(|step| claude_step(step, 0))
        .collect();
    lines.push(init);
    lines.extend(
        steps
            .into_iter()
            .enumerate()
            .flat_map(|(index, step)| claude_step(step, index + 1)),
    );
    lines.extend(claude_outcome(&script.outcome));
    lines.iter().map(Value::to_string).collect()
}

/// The steps a stream prints before its session opens, and the steps after.
fn split_open(steps: &[Step]) -> (Vec<&Step>, Vec<&Step>) {
    steps
        .iter()
        .partition(|step| matches!(step, Step::BeforeOpen(_)))
}

/// The lines a Claude CLI prints for one step. `n` numbers the step so that
/// every tool call and message it prints has an id of its own.
fn claude_step(step: &Step, n: usize) -> Vec<Value> {
    match step {
        Step::Note(text) | Step::BeforeOpen(text) => {
            vec![claude_message(json!({ "content": [text_block(text)] }))]
        }
        Step::StopRefused { reason } => {
            vec![claude_message(
                json!({ "content": [text_block(&format!("{STOP_TURN}{reason}"))] }),
            )]
        }
        Step::UnknownLine => claude_unread(),
        Step::ToolUse { name, target } => vec![claude_message(json!({
            "content": [{
                "type": "tool_use",
                "id": format!("toolu_{n}"),
                "name": name,
                "input": { "command": target },
            }],
        }))],
        Step::RunToolCalled(tool) => vec![claude_message(json!({
            "content": [{
                "type": "tool_use",
                "id": format!("toolu_{n}"),
                "name": claude_run_tool(*tool),
                "input": {},
            }],
        }))],
        Step::Usage {
            input,
            output,
            cached,
        } => vec![claude_message(json!({
            "id": format!("msg_{n}"),
            "content": [],
            "usage": claude_usage(*input, *output, *cached),
        }))],
    }
}

/// The lines of the kinds a Claude CLI prints that its stream reader does not
/// read: a context compaction, a user line and a status line, each as the CLI
/// writes it.
fn claude_unread() -> Vec<Value> {
    vec![
        json!({ "type": "compact_boundary", "reason": "context" }),
        json!({ "type": "user", "message": { "content": [] } }),
        json!({ "type": "system", "subtype": "status", "note": "still here" }),
    ]
}

/// An assistant line carrying one message.
fn claude_message(message: Value) -> Value {
    json!({ "type": "assistant", "message": message })
}

/// A text block of an assistant message.
fn text_block(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

/// The name a Claude CLI prints for a call to a run tool: the server's name
/// after `mcp__`, then the tool's own name.
fn claude_run_tool(tool: RunTool) -> String {
    format!("mcp__{}__{}", RunToolsEndpoint::SERVER_NAME, tool.name())
}

/// A message's usage in the shape Claude reports it: the input it read
/// outside the cache, then the cache reads, which are the cached part of the
/// input. Absent cached tokens stay absent.
fn claude_usage(input: u64, output: u64, cached: Option<u64>) -> Value {
    let mut usage = json!({ "input_tokens": input, "output_tokens": output });
    if let Some(cached) = cached {
        usage["input_tokens"] = json!(input.saturating_sub(cached));
        usage["cache_read_input_tokens"] = json!(cached);
    }
    usage
}

/// The terminal line a Claude CLI prints for an outcome; a crash prints none.
fn claude_outcome(outcome: &Outcome) -> Option<Value> {
    match outcome {
        Outcome::Completed(summary) => {
            Some(json!({ "type": "result", "is_error": false, "result": summary }))
        }
        Outcome::Failed(message) => {
            Some(json!({ "type": "result", "is_error": true, "result": message }))
        }
        Outcome::Crash => None,
    }
}

/// The JSON lines a Codex CLI prints for `script`: the lines it prints before
/// its thread starts, the thread start, one item per step, the final message,
/// and the turn's completion with its usage.
pub fn codex_lines(script: &Script) -> Vec<String> {
    let (before_open, steps) = split_open(&script.steps);
    let mut lines: Vec<Value> = before_open.into_iter().flat_map(codex_step).collect();
    lines.push(json!({ "type": "thread.started", "thread_id": CODEX_THREAD }));
    lines.extend(steps.into_iter().flat_map(codex_step));
    match &script.outcome {
        Outcome::Completed(summary) => {
            lines.push(agent_message(summary));
            let mut turn = json!({ "type": "turn.completed" });
            if let Some(usage) = codex_usage(&spent(&script.steps)) {
                turn["usage"] = usage;
            }
            lines.push(turn);
        }
        Outcome::Failed(message) => {
            lines.push(json!({ "type": "turn.failed", "error": { "message": message } }));
        }
        Outcome::Crash => {}
    }
    lines.iter().map(Value::to_string).collect()
}

/// The lines a Codex CLI prints for one step. Usage is reported at the turn's
/// end, so a usage step prints none of its own.
fn codex_step(step: &Step) -> Vec<Value> {
    match step {
        Step::Note(text) | Step::BeforeOpen(text) => vec![agent_message(text)],
        Step::StopRefused { reason } => vec![agent_message(&format!("{STOP_TURN}{reason}"))],
        Step::UnknownLine => codex_unread(),
        Step::ToolUse { target, .. } => vec![completed_item(json!({
            "type": "command_execution",
            "command": target,
            "status": "completed",
        }))],
        Step::RunToolCalled(tool) => vec![completed_item(json!({
            "type": "mcp_tool_call",
            "server": RunToolsEndpoint::SERVER_NAME,
            "tool": tool.name(),
            "status": "completed",
        }))],
        Step::Usage { .. } => Vec::new(),
    }
}

/// The lines of the kinds a Codex CLI prints that its stream reader does not
/// read: a thread rename and a turn start, each as the CLI writes it.
fn codex_unread() -> Vec<Value> {
    vec![
        json!({ "type": "thread.renamed", "name": "something new" }),
        json!({ "type": "turn.started" }),
    ]
}

/// The usage steps of a script, as the turn's totals are summed from them.
fn spent(steps: &[Step]) -> Vec<Spent> {
    steps
        .iter()
        .filter_map(|step| match step {
            Step::Usage {
                input,
                output,
                cached,
            } => Some((*input, *output, *cached)),
            _ => None,
        })
        .collect()
}

/// The usage a turn reports: the sums of the usage it spent, with the cached
/// part present only when some step reported one. A turn that spent nothing
/// reports no usage.
fn codex_usage(spent: &[Spent]) -> Option<Value> {
    if spent.is_empty() {
        return None;
    }
    let input: u64 = spent.iter().map(|(input, _, _)| input).sum();
    let output: u64 = spent.iter().map(|(_, output, _)| output).sum();
    let cached = spent
        .iter()
        .filter_map(|(_, _, cached)| *cached)
        .fold(None, |sum: Option<u64>, cached| {
            Some(sum.unwrap_or(0) + cached)
        });
    let mut usage = json!({ "input_tokens": input, "output_tokens": output });
    if let Some(cached) = cached {
        usage["cached_input_tokens"] = json!(cached);
    }
    Some(usage)
}

/// A completed item event, as a Codex CLI prints it.
fn completed_item(item: Value) -> Value {
    json!({ "type": "item.completed", "item": item })
}

/// An agent message item, as a Codex CLI prints the text it says.
fn agent_message(text: &str) -> Value {
    completed_item(json!({ "type": "agent_message", "text": text }))
}
