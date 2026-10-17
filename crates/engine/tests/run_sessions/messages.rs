//! The agent messages a session leaves on the log: a note as its size and digest,
//! and the usage and tool calls as the adapter reported them.

use super::*;

#[tokio::test]
async fn agent_messages_are_bounded_summaries_that_never_carry_note_content() {
    let bench = Bench::new();
    let fixture = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: note, text: "thinking about SECRET-TOKEN-123 carefully" }
      - { type: usage, input_tokens: 40, output_tokens: 10 }
      - { type: tool_use, name: edit, target: abc123 }
    outcome: { type: completed, summary: "done" }
"#;
    let RunReport { terminal, state: _ } = bench.run(SESSION_EVENTS_WORKFLOW, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.events();
    let messages: Vec<&yunta_core::events::AgentMessagePayload> = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Session(SessionEvent::Message(p))) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 3, "one agent_message per adapter event");

    assert_note_is_a_digest(&messages);
    assert_usage_and_tool_call_are_kept(&messages);
}

/// Never the content: the note keeps its size and digest, and no text.
fn assert_note_is_a_digest(messages: &[&yunta_core::events::AgentMessagePayload]) {
    let jsonl = serde_json::to_string(&messages).unwrap();
    assert!(
        !jsonl.contains("SECRET-TOKEN-123"),
        "note content must never be persisted: {jsonl}"
    );
    let note = messages
        .iter()
        .find(|m| m.message_type == yunta_core::events::AgentMessageType::Note)
        .unwrap();
    assert_eq!(
        note.note,
        Some(yunta_core::events::NoteDigest {
            bytes: 41,
            sha256: "052f9656f4d7".to_string(),
        })
    );
    assert!(note.text.is_none(), "{note:?}");
}

/// The usage and the tool call keep what the adapter reported; a tool's
/// argument is identified by its digest and never shown.
fn assert_usage_and_tool_call_are_kept(messages: &[&yunta_core::events::AgentMessagePayload]) {
    let usage = messages
        .iter()
        .find(|m| m.message_type == yunta_core::events::AgentMessageType::Usage)
        .unwrap();
    assert_eq!(usage.input_tokens, Some(40));
    assert_eq!(usage.output_tokens, Some(10));

    let tool = messages
        .iter()
        .find(|m| m.message_type == yunta_core::events::AgentMessageType::ToolUse)
        .unwrap();
    assert_eq!(tool.tool_name.as_deref(), Some("edit"));
    assert_eq!(
        tool.target.as_ref().map(|target| target.digest().clone()),
        Some(yunta_core::sha256_hex(b"abc123")),
        "the mock hands over what the fixture scripted, identified by its hash"
    );
    assert_eq!(
        tool.target.as_ref().and_then(|target| target.display()),
        None,
        "and never shown: a tool's argument is the session's own text"
    );
}
