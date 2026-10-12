//! The lines the `codex` CLI prints for a session, as the mock plays it: one
//! fixture step as an item of its stream, the line that opens the session,
//! and the stdin its hooks are sent. Each line is written from the CLI's own
//! format and never read back from this adapter's parser, so the reader is
//! checked against the format rather than against itself.

use std::path::Path;

use serde_json::{json, Value};
use yunta_core::port::RunToolsEndpoint;
use yunta_core::rail::RailEvent;
use yunta_core::{ModelName, RunTool, SessionId, ToolNaming};

use crate::mock::MockStep;

/// The lines the CLI prints for one step of a fixture, in the order it
/// prints them.
pub(crate) fn render(step: &MockStep, naming: ToolNaming) -> Vec<String> {
    match step {
        MockStep::Note { text, .. } | MockStep::BeforeOpen { text, .. } => {
            vec![item(json!({ "type": "agent_message", "text": text }))]
        }
        // Kinds the stream carries that the reader does not read, as the CLI writes them.
        MockStep::UnknownLine { .. } => vec![
            json!({ "type": "thread.renamed", "name": "something new" }).to_string(),
            json!({ "type": "turn.started" }).to_string(),
        ],
        MockStep::ToolUse { name, target, .. } => vec![tool_use(name, target, naming)],
        MockStep::RunTool { tool, .. } => vec![run_tool_call(tool)],
        MockStep::Usage {
            input_tokens,
            output_tokens,
            cached_input_tokens,
            ..
        } => vec![turn_completed(
            *input_tokens,
            *output_tokens,
            *cached_input_tokens,
        )],
    }
}

/// The line that opens the session: `thread.started`, which names the thread
/// and no model. The CLI's stream names no model, so none is written.
pub(crate) fn open(session_id: &SessionId, _model: Option<&ModelName>) -> Vec<String> {
    vec![json!({ "type": "thread.started", "thread_id": session_id.as_str() }).to_string()]
}

/// The stdin a hook of this CLI is sent for `event`: the session it belongs
/// to and, for a write, the `apply_patch` call that names `path`.
pub(crate) fn hook_stdin(event: RailEvent, session: &SessionId, path: &Path) -> Vec<u8> {
    let call = match event {
        RailEvent::BeforeStop => {
            json!({ "session_id": session.as_str(), "hook_event_name": "Stop" })
        }
        RailEvent::BeforeWrite | RailEvent::AfterWrite => json!({
            "session_id": session.as_str(),
            "tool_name": "apply_patch",
            "tool_input": { "command": patch_naming(path) },
        }),
    };
    call.to_string().into_bytes()
}

/// One tool call. A run tool of this run's server is the MCP call the CLI
/// makes for it; every other call is a shell call, which the CLI prints as a
/// `command_execution` item.
fn tool_use(name: &str, target: &str, naming: ToolNaming) -> String {
    match RunTool::from_called(name, naming) {
        Some(tool) => run_tool_call(tool.name()),
        None => item(json!({
            "type": "command_execution",
            "command": target,
            "aggregated_output": "",
            "exit_code": 0,
            "status": "completed",
        })),
    }
}

/// A completed call of a run tool, as the MCP call of this run's server.
fn run_tool_call(tool: &str) -> String {
    item(json!({
        "type": "mcp_tool_call",
        "server": RunToolsEndpoint::SERVER_NAME,
        "tool": tool,
        "status": "completed",
    }))
}

/// One completed item of the stream.
fn item(payload: Value) -> String {
    json!({ "type": "item.completed", "item": payload }).to_string()
}

/// The turn's end, which carries the usage the CLI counted. A count the CLI
/// did not report is left out, never written as zero.
fn turn_completed(input: u64, output: u64, cached: Option<u64>) -> String {
    let mut usage = json!({ "input_tokens": input, "output_tokens": output });
    if let (Some(cached), Some(map)) = (cached, usage.as_object_mut()) {
        map.insert("cached_input_tokens".to_string(), json!(cached));
    }
    json!({ "type": "turn.completed", "usage": usage }).to_string()
}

/// A patch that updates the one file `path` names, in the form the codec
/// reads its paths from.
fn patch_naming(path: &Path) -> String {
    format!(
        "*** Begin Patch\n*** Update File: {}\n*** End Patch",
        path.display()
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use yunta_core::fence::Coverage;
    use yunta_core::port::{AgentEvent, RailCodec};
    use yunta_core::rail::{RailAct, RailCall};

    use super::*;
    use crate::codex::parse::parse_line;
    use crate::codex::rail::CodexRailCodec;

    /// The events the reader makes of `lines`, one line at a time.
    fn reads_back(lines: &[String]) -> Vec<AgentEvent> {
        lines
            .iter()
            .flat_map(|line| parse_line(line, "", &Coverage::Exact))
            .collect()
    }

    #[test]
    fn each_step_reads_back_as_the_event_it_describes() {
        let naming = ToolNaming::McpPrefixedUnderscored;
        let note = MockStep::Note {
            text: "all done".to_string(),
            after_ms: 0,
        };
        assert!(matches!(
            reads_back(&render(&note, naming)).as_slice(),
            [AgentEvent::Note { text }] if text == "all done"
        ));
        let shell = MockStep::ToolUse {
            name: "command_execution".to_string(),
            target: "cargo test".to_string(),
            after_ms: 0,
        };
        assert!(matches!(
            reads_back(&render(&shell, naming)).as_slice(),
            [AgentEvent::ToolUse { name, .. }] if name == "command_execution"
        ));
        let run = MockStep::ToolUse {
            name: RunTool::CheckArtifact.called(naming),
            target: "ignored".to_string(),
            after_ms: 0,
        };
        assert!(matches!(
            reads_back(&render(&run, naming)).as_slice(),
            [AgentEvent::ToolUse { name, .. }] if name == RunTool::CheckArtifact.name()
        ));
        let usage = MockStep::Usage {
            input_tokens: 10,
            output_tokens: 4,
            cached_input_tokens: Some(2),
            after_ms: 0,
        };
        assert!(matches!(
            reads_back(&render(&usage, naming)).first(),
            Some(AgentEvent::Usage {
                input_tokens: Some(10),
                output_tokens: Some(4),
                cached_input_tokens: Some(2),
            })
        ));
        let session: SessionId = "thread-abc".parse().unwrap();
        let opened = open(&session, None);
        assert!(matches!(
            reads_back(&opened).as_slice(),
            [AgentEvent::SessionOpened { model: None, .. }]
        ));
    }

    #[test]
    fn a_write_hook_is_sent_the_call_that_names_its_path() {
        let session: SessionId = "thread-abc".parse().unwrap();
        let stdin = hook_stdin(RailEvent::BeforeWrite, &session, Path::new("src/lib.rs"));
        let call = CodexRailCodec.decode(RailEvent::BeforeWrite, &stdin);
        assert!(matches!(
            call,
            Ok(Some(RailCall { act: RailAct::BeforeWrite { paths }, .. }))
                if paths == [PathBuf::from("src/lib.rs")]
        ));
    }
}
