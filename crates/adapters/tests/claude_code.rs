//! Integration tests for the real `claude-code` adapter (T7.3) against a
//! fake `claude` binary (`fixtures/claude_code_stub.sh`) — no network, no
//! API cost, no real LLM in CI (A8). The one thing this suite cannot
//! cover is whether the real CLI's actual output matches what the stub
//! scripts: that is the manual smoke test `docs/m0-status.md` records.
//!
//! Stub configuration travels through `SessionRequest.env` (the child
//! process's own environment), never `std::env::set_var` on the test
//! binary itself — cargo runs tests concurrently in one process, and a
//! process-global env var would race across them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use yunta_adapters::{
    Adapter, AgentEvent, Budget, ClaudeCodeAdapter, PermissionProfile, SessionRequest,
};
use yunta_core::{AdapterSettings, SessionId};

fn stub_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude_code_stub.sh")
}

fn adapter() -> ClaudeCodeAdapter {
    ClaudeCodeAdapter::new(&AdapterSettings {
        binary: Some(stub_path()),
    })
}

fn request(cwd: PathBuf) -> SessionRequest {
    SessionRequest {
        prompt: "do the thing".to_string(),
        cwd,
        model: None,
        agent: None,
        permissions: PermissionProfile::Edit,
        env: HashMap::new(),
        edit_constraints: None,
        budget: Budget::default(),
        adapter_settings: serde_json::Map::new(),
    }
}

async fn drain(mut session: Box<dyn yunta_adapters::AgentSession>) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    let mut stream = session.events();
    while let Some(event) = stream.next().await {
        events.push(event);
    }
    events
}

fn write_lines(dir: &std::path::Path, name: &str, lines: &[&str]) -> PathBuf {
    let path = dir.join(name);
    let contents = if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    };
    std::fs::write(&path, contents).unwrap();
    path
}

const INIT_LINE: &str =
    r#"{"type":"system","subtype":"init","session_id":"sess-abc","model":"claude-sonnet-5"}"#;

#[tokio::test]
async fn probe_reports_the_stub_as_healthy_with_its_version() {
    let report = adapter().probe().await.unwrap();
    assert!(report.healthy);
    assert!(report.version.unwrap().contains("2.1.235"));
}

#[tokio::test]
async fn capabilities_declare_what_this_adapter_actually_does() {
    let caps = adapter().capabilities();
    assert!(caps.resume_session);
    assert!(caps.permission_profiles);
    assert!(caps.custom_agents);
    assert!(caps.usage_reporting);
    // A6: never claim a capability that isn't wired end-to-end yet — M-0
    // has no live edit-hook blocking for the real CLI, only the engine's
    // post-hoc scope check (T5.3).
    assert!(!caps.edit_hooks);
    assert!(!caps.run_tools);
}

#[tokio::test]
async fn a_successful_session_opens_streams_usage_and_completes() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            INIT_LINE,
            r#"{"type":"assistant","message":{"id":"msg-1","content":[{"type":"text","text":"working on it"}]}}"#,
            r#"{"type":"result","is_error":false,"result":"all done","usage":{"input_tokens":10,"output_tokens":4,"cache_read_input_tokens":2}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(matches!(
        &events[0],
        AgentEvent::SessionOpened { model, .. } if model == "claude-sonnet-5"
    ));
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::Note { text } if text == "working on it")));
    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::Usage {
            input_tokens: 10,
            output_tokens: 4,
            cached_input_tokens: Some(2)
        }
    )));
    assert!(matches!(
        events.last().unwrap(),
        AgentEvent::Completed { result } if result.summary == "all done"
    ));
}

#[tokio::test]
async fn a_failed_result_ends_the_stream_with_failed_and_retryable() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            INIT_LINE,
            r#"{"type":"result","is_error":true,"result":"rate limited","usage":{"input_tokens":5,"output_tokens":0}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    match events.last().unwrap() {
        AgentEvent::Failed { error, retryable } => {
            assert_eq!(error.message, "rate limited");
            assert!(*retryable);
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[tokio::test]
async fn a_tool_use_block_maps_to_tool_use_with_a_readable_digest() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            INIT_LINE,
            r#"{"type":"assistant","message":{"id":"msg-1","content":[{"type":"tool_use","name":"Edit","input":{"file_path":"src/lib.rs"}}]}}"#,
            r#"{"type":"result","is_error":false,"result":"edited","usage":{"input_tokens":1,"output_tokens":1}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::ToolUse { name, target_digest }
            if name == "Edit" && target_digest == "src/lib.rs"
    )));
}

#[tokio::test]
async fn a_crashed_session_ends_the_stream_with_no_terminal_event() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(dir.path(), "lines.jsonl", &[INIT_LINE]);

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], AgentEvent::SessionOpened { .. }));
}

#[tokio::test]
async fn read_only_restricts_the_tool_set_and_never_asks() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.permissions = PermissionProfile::ReadOnly;
    req.env.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;

    let args = std::fs::read_to_string(&args_file).unwrap();
    assert!(args.contains("--tools"), "got: {args}");
    assert!(args.contains("acceptEdits"), "got: {args}");
}

#[tokio::test]
async fn edit_and_full_run_unattended_without_the_root_blocked_flags() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.permissions = PermissionProfile::Full;
    req.env.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;

    let args = std::fs::read_to_string(&args_file).unwrap();
    assert!(args.contains("acceptEdits"), "got: {args}");
    // Both are refused when the CLI runs as root — confirmed empirically
    // (see permissions.rs) — so neither may appear in the built args.
    assert!(
        !args.contains("--dangerously-skip-permissions"),
        "got: {args}"
    );
    assert!(!args.contains("bypassPermissions"), "got: {args}");
}

#[tokio::test]
async fn model_and_agent_are_passed_through_as_their_own_flags() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.model = Some("claude-opus-5".to_string());
    req.agent = Some("benito".to_string());
    req.env.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;

    let args: Vec<String> = std::fs::read_to_string(&args_file)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    let model_pos = args.iter().position(|a| a == "--model").unwrap();
    assert_eq!(args[model_pos + 1], "claude-opus-5");
    let agent_pos = args.iter().position(|a| a == "--agent").unwrap();
    assert_eq!(args[agent_pos + 1], "benito");
}

#[tokio::test]
async fn resuming_passes_the_session_id_to_the_resume_flag() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session_id = SessionId::from("sess-to-resume");
    let session = adapter().resume(&session_id, req).await.unwrap();
    let _ = drain(session).await;

    let args: Vec<String> = std::fs::read_to_string(&args_file)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    let resume_pos = args.iter().position(|a| a == "--resume").unwrap();
    assert_eq!(args[resume_pos + 1], "sess-to-resume");
}

#[tokio::test]
async fn kill_terminates_the_whole_process_tree_including_grandchildren() {
    let dir = tempfile::tempdir().unwrap();
    let child_pid_file = dir.path().join("child.pid");

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CLAUDE_STUB_CHILD_PID_FILE".to_string(),
        child_pid_file.display().to_string(),
    );
    req.env
        .insert("CLAUDE_STUB_HANG".to_string(), "1".to_string());
    let mut session = adapter().spawn(req).await.unwrap();

    // Give the stub a moment to record its grandchild's pid.
    for _ in 0..50 {
        if child_pid_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let grandchild_pid = std::fs::read_to_string(&child_pid_file)
        .unwrap()
        .trim()
        .to_string();

    session.kill().await.unwrap();

    // A4: the grandchild the stub spawned must die too, not just the
    // stub itself — proving the kill reached the whole process group.
    // `kill -0` alone isn't enough here: a killed process whose parent
    // is also gone lingers as a zombie (still `kill -0`-visible) until
    // something reaps it, so a live, running process is specifically
    // one `ps` still reports a state for that isn't `Z` (zombie).
    let mut grandchild_running = true;
    for _ in 0..50 {
        let ps = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &grandchild_pid])
            .output()
            .unwrap();
        let state = String::from_utf8_lossy(&ps.stdout);
        let state = state.trim();
        if state.is_empty() || state.starts_with('Z') {
            grandchild_running = false;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!grandchild_running, "grandchild process survived kill()");
}
