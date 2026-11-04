//! What a tool call does, by the word a reader is told, and by its tool's name
//! in a log older than the word.

use yunta_core::events::ToolAction::{
    self, Call, Edit, Fetch, List, Read, Run, Search, Submit, Write,
};
use yunta_core::events::{AgentMessagePayload, ToolTarget};

#[test]
fn each_action_has_the_word_a_reader_is_told() {
    let table = [
        (Read, "read"),
        (Edit, "edit"),
        (Write, "write"),
        (Search, "search"),
        (List, "list"),
        (Run, "run"),
        (Fetch, "fetch"),
        (Call, "call"),
        (Submit, "submit"),
    ];
    for (action, word) in table {
        assert_eq!(action.as_str(), word);
    }
}

#[test]
fn a_tool_is_read_by_its_own_name_under_every_cli() {
    let table: &[(&str, ToolAction)] = &[
        ("Read", Read),
        ("Edit", Edit),
        ("MultiEdit", Edit),
        ("NotebookEdit", Edit),
        ("file_change", Edit),
        ("Write", Write),
        ("Grep", Search),
        ("WebSearch", Search),
        ("web_search", Search),
        ("Glob", List),
        ("LS", List),
        ("Bash", Run),
        ("command_execution", Run),
        ("WebFetch", Fetch),
        ("mcp_tool_call", Call),
        ("Task", Call),
        ("yunta_check_artifact", Call),
        ("yunta_submit_tasks", Submit),
    ];
    for (name, action) in table {
        assert_eq!(ToolAction::of_tool(name), *action, "{name}");
    }
}

/// A line as a log older than the word holds it.
fn old_line(tool: &str) -> AgentMessagePayload {
    let target = serde_json::to_value(ToolTarget::opaque(b"ls")).unwrap();
    serde_json::from_value(serde_json::json!({
        "message_type": "tool_use",
        "tool_name": tool,
        "target": target,
    }))
    .unwrap()
}

#[test]
fn a_log_from_before_actions_reads_by_the_tools_name() {
    let payload = old_line("Bash");
    assert_eq!(payload.action, None);
    assert_eq!(payload.tool_action(), Run);
    assert_eq!(old_line("Read").tool_action(), Read);
    assert_eq!(old_line("a_tool_of_its_own").tool_action(), Call);
}

#[test]
fn what_an_adapter_read_wins_over_the_name_and_absent_it_is_not_written() {
    let mut payload = old_line("Bash");
    let written = serde_json::to_value(&payload).unwrap();
    assert!(written.get("action").is_none(), "{written}");
    payload.action = Some(Read);
    assert_eq!(payload.tool_action(), Read);
    let written = serde_json::to_value(&payload).unwrap();
    assert_eq!(written["action"], "read");
}

#[test]
fn a_piece_it_vouches_for_is_shown_and_the_hash_stays_of_the_whole_text() {
    let target = ToolTarget::showing("cargo test", b"cargo test --token hunter2");
    assert_eq!(target.display(), Some("cargo test"));
    assert_eq!(
        *target.digest(),
        yunta_core::sha256_hex(b"cargo test --token hunter2")
    );
}
