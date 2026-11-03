//! The line a tool call reads as, off a terminal.

use yunta_core::events::{AgentMessagePayload, EventPayload, SessionEvent};

use super::said_for;

/// A call reads as what it did, and what the log cannot show stays out of it,
/// whether its adapter read the action or a log older than the word has only
/// the tool's name.
#[test]
fn a_tool_call_reads_as_its_action() {
    use std::path::Path;
    use yunta_core::events::{AgentMessageType, ToolAction, ToolTarget};
    let said = |action: ToolAction, tool: &str, target: ToolTarget| {
        said_for(EventPayload::Session(SessionEvent::Message(
            AgentMessagePayload {
                message_type: AgentMessageType::ToolUse,
                tool_name: Some(tool.to_string()),
                action: Some(action),
                target: Some(target),
                input_tokens: None,
                output_tokens: None,
                cached_input_tokens: None,
                note: None,
                text: None,
            },
        )))
    };
    let path = ToolTarget::of_path(Path::new("src/lib.rs"));
    assert_eq!(said(ToolAction::Read, "Read", path), "run: read src/lib.rs");
    assert_eq!(
        said(ToolAction::Run, "Bash", ToolTarget::named("cargo test")),
        "run: run cargo test"
    );
    let secret = ToolTarget::opaque(b"psql postgres://hunter2");
    assert_eq!(said(ToolAction::Run, "Bash", secret.clone()), "run: run");
    assert_eq!(
        said(ToolAction::Call, "Task", secret.clone()),
        "run: call Task"
    );

    let old: AgentMessagePayload = serde_json::from_value(serde_json::json!({
        "message_type": "tool_use",
        "tool_name": "Bash",
        "target": serde_json::to_value(&secret).unwrap(),
    }))
    .unwrap();
    assert_eq!(
        said_for(EventPayload::Session(SessionEvent::Message(old))),
        "run: run"
    );
}
