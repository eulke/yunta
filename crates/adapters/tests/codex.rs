//! Integration tests for the real `codex` adapter (T7.4) against a fake
//! `codex` binary (`fixtures/codex_stub.sh`) — no network, no API cost,
//! no real LLM in CI (A8). What this suite cannot cover — whether the
//! real CLI's actual output matches what the stub scripts — has no
//! manual smoke test to fall back on either: no `codex` binary or
//! credentials exist in this environment. See `codex/mod.rs`'s own doc
//! comment and `docs/m0-status.md`'s T7.4 entry.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use yunta_adapters::{
    Adapter, AgentEvent, Budget, CodexAdapter, PermissionProfile, SessionRequest,
};
use yunta_core::{AdapterSettings, SessionId};

fn stub_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex_stub.sh")
}

fn adapter() -> CodexAdapter {
    CodexAdapter::new(&AdapterSettings {
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
        skills: Vec::new(),
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

const THREAD_STARTED_LINE: &str = r#"{"type":"thread.started","thread_id":"thread-abc"}"#;

#[tokio::test]
async fn probe_reports_the_stub_as_healthy_with_its_version() {
    let report = adapter().probe().await.unwrap();
    assert!(report.healthy);
    assert!(report.version.unwrap().contains("0.47.0"));
}

#[tokio::test]
async fn capabilities_declare_what_this_adapter_actually_does() {
    let caps = adapter().capabilities();
    assert!(caps.resume_session);
    assert!(caps.permission_profiles);
    assert!(caps.usage_reporting);
    // A6: never claim a capability that isn't wired end-to-end yet.
    assert!(!caps.custom_agents);
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
            THREAD_STARTED_LINE,
            r#"{"type":"turn.started"}"#,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"all done"}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":4}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.model = Some("gpt-5-codex".to_string());
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(matches!(
        &events[0],
        AgentEvent::SessionOpened { model, .. } if model == "gpt-5-codex"
    ));
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::Note { text } if text == "all done")));
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
async fn a_failed_turn_ends_the_stream_with_failed_and_retryable() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            THREAD_STARTED_LINE,
            r#"{"type":"turn.failed","error":{"message":"model response stream ended unexpectedly"}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    match events.last().unwrap() {
        AgentEvent::Failed { error, retryable } => {
            assert_eq!(error.message, "model response stream ended unexpectedly");
            assert!(*retryable);
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[tokio::test]
async fn a_command_execution_item_maps_to_tool_use_with_the_command_as_digest() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            THREAD_STARTED_LINE,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"command_execution","command":"cargo test","aggregated_output":"ok","exit_code":0,"status":"completed"}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::ToolUse { name, target_digest }
            if name == "command_execution" && target_digest == "cargo test"
    )));
}

#[tokio::test]
async fn a_file_change_item_maps_to_tool_use_with_the_first_path_as_digest() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            THREAD_STARTED_LINE,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"file_change","changes":[{"path":"src/lib.rs","kind":"update"},{"path":"src/main.rs","kind":"update"}],"status":"completed"}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::ToolUse { name, target_digest }
            if name == "file_change" && target_digest == "src/lib.rs"
    )));
}

#[tokio::test]
async fn an_mcp_tool_call_item_maps_to_tool_use_with_server_and_tool_as_digest() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            THREAD_STARTED_LINE,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"mcp_tool_call","server":"yunta","tool":"query","status":"completed"}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::ToolUse { name, target_digest }
            if name == "mcp_tool_call" && target_digest == "yunta:query"
    )));
}

#[tokio::test]
async fn a_web_search_item_maps_to_tool_use_with_the_query_as_digest() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            THREAD_STARTED_LINE,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"web_search","query":"codex exec json schema","action":"search"}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::ToolUse { name, target_digest }
            if name == "web_search" && target_digest == "codex exec json schema"
    )));
}

#[tokio::test]
async fn a_reasoning_item_is_never_surfaced() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            THREAD_STARTED_LINE,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"thinking it through"}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert_eq!(
        events.len(),
        1,
        "only SessionOpened, nothing from reasoning: {events:?}"
    );
}

#[tokio::test]
async fn a_crashed_session_ends_the_stream_with_no_terminal_event() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(dir.path(), "lines.jsonl", &[THREAD_STARTED_LINE]);

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], AgentEvent::SessionOpened { .. }));
}

#[tokio::test]
async fn a_session_with_no_requested_model_falls_back_to_a_named_default() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(dir.path(), "lines.jsonl", &[THREAD_STARTED_LINE]);

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(matches!(
        &events[0],
        AgentEvent::SessionOpened { model, .. } if model == "default"
    ));
}

#[tokio::test]
async fn each_permission_profile_maps_to_its_own_sandbox_mode() {
    for (profile, expected) in [
        (PermissionProfile::ReadOnly, "read-only"),
        (PermissionProfile::Edit, "workspace-write"),
        (PermissionProfile::Full, "danger-full-access"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let args_file = dir.path().join("args.txt");
        let lines = write_lines(dir.path(), "lines.jsonl", &[]);

        let mut req = request(dir.path().to_path_buf());
        req.permissions = profile;
        req.env.insert(
            "CODEX_STUB_ARGS_FILE".to_string(),
            args_file.display().to_string(),
        );
        req.env.insert(
            "CODEX_STUB_LINES_FILE".to_string(),
            lines.display().to_string(),
        );
        let session = adapter().spawn(req).await.unwrap();
        let _ = drain(session).await;

        let args: Vec<String> = std::fs::read_to_string(&args_file)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        let pos = args.iter().position(|a| a == "--sandbox").unwrap();
        assert_eq!(args[pos + 1], expected);
    }
}

#[tokio::test]
async fn model_is_passed_through_as_its_own_flag() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.model = Some("gpt-5-codex".to_string());
    req.env.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
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
    assert_eq!(args[model_pos + 1], "gpt-5-codex");
}

#[tokio::test]
async fn resuming_passes_the_thread_id_to_the_resume_subcommand() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session_id = SessionId::from("thread-to-resume");
    let session = adapter().resume(&session_id, req).await.unwrap();
    let _ = drain(session).await;

    let args: Vec<String> = std::fs::read_to_string(&args_file)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    let resume_pos = args.iter().position(|a| a == "resume").unwrap();
    assert_eq!(args[resume_pos + 1], "thread-to-resume");
}

#[tokio::test]
async fn kill_terminates_the_whole_process_tree_including_grandchildren() {
    let dir = tempfile::tempdir().unwrap();
    let child_pid_file = dir.path().join("child.pid");

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_CHILD_PID_FILE".to_string(),
        child_pid_file.display().to_string(),
    );
    req.env
        .insert("CODEX_STUB_HANG".to_string(), "1".to_string());
    let mut session = adapter().spawn(req).await.unwrap();

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
