//! The messages a session sends while a run is in progress: the token usage it
//! reports and the tool calls it makes. The derivations read them off the log.

use super::*;

pub(super) fn usage(input: u64, output: u64) -> EventPayload {
    EventPayload::Session(SessionEvent::Message(AgentMessagePayload {
        message_type: AgentMessageType::Usage,
        tool_name: None,
        target: None,
        input_tokens: Some(input),
        output_tokens: Some(output),
        cached_input_tokens: None,
        note: None,
        text: None,
    }))
}

pub(super) fn tool_use(tool: &str, digest: &str) -> EventPayload {
    EventPayload::Session(SessionEvent::Message(AgentMessagePayload {
        message_type: AgentMessageType::ToolUse,
        tool_name: Some(tool.to_string()),
        target: Some(yunta_core::events::ToolTarget::opaque(digest.as_bytes())),
        input_tokens: None,
        output_tokens: None,
        cached_input_tokens: None,
        note: None,
        text: None,
    }))
}
