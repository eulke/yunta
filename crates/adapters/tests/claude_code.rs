//! Integration tests for the real `claude-code` adapter against a fake
//! `claude` binary (`fixtures/claude_code_stub.sh`) — no network, no
//! API cost, no real LLM in CI. The one thing this suite cannot cover
//! is whether the real CLI's actual output matches what the stub
//! scripts: that is what the manual smoke test covers instead.
//!
//! Stub configuration travels through `SessionRequest.env` (the child
//! process's own environment), never by mutating the process environment
//! in a test — cargo runs tests concurrently in one process, and a
//! process-global env var would race across them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use yunta_adapters::{
    Adapter, AgentEvent, Budget, ClaudeCodeAdapter, PermissionProfile, ProbeReport, SessionRequest,
};
use yunta_core::{AdapterSettings, SessionId};

fn stub_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude_code_stub.sh")
}

fn adapter() -> ClaudeCodeAdapter {
    ClaudeCodeAdapter::new(&AdapterSettings {
        adapter_settings: None,
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
        run_tools_endpoint: None,
        artifact_dir: None,
        scratch_dir: None,
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
    let ProbeReport::Healthy { version } = report else {
        panic!("the stub is healthy: {report:?}");
    };
    assert!(version.unwrap().contains("2.1.235"));
}

#[tokio::test]
async fn capabilities_declare_what_this_adapter_actually_does() {
    let caps = adapter().capabilities();
    assert!(caps.resume_session);
    assert!(caps.permission_profiles);
    assert!(caps.custom_agents);
    assert!(caps.usage_reporting);
    // Never claim a capability that isn't wired end-to-end yet — there
    // is no live edit-hook blocking for the real CLI, only the engine's
    // post-hoc scope check.
    assert!(!caps.edit_hooks);
    assert!(caps.run_tools);
}

#[tokio::test]
async fn capability_usage_reporting_surfaces_the_streams_usage() {
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
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(matches!(
        &events[0],
        AgentEvent::SessionOpened { model, .. } if model.as_ref().is_some_and(|model| model == "claude-sonnet-5")
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
        lines.display().to_string().into(),
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
        lines.display().to_string().into(),
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
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], AgentEvent::SessionOpened { .. }));
}

#[tokio::test]
async fn capability_permission_profiles_give_read_only_the_non_mutating_tools() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.permissions = PermissionProfile::ReadOnly;
    req.env.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;

    let args: Vec<String> = std::fs::read_to_string(&args_file)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    let tools_pos = args
        .iter()
        .position(|a| a == "--tools")
        .expect("ReadOnly restricts the tools");
    assert_eq!(
        args[tools_pos + 1],
        "Read,Grep,Glob,WebFetch,WebSearch,Write"
    );
    let mode_pos = args
        .iter()
        .position(|a| a == "--permission-mode")
        .expect("every profile sets an unattended permission mode");
    assert_eq!(args[mode_pos + 1], "acceptEdits");
}

#[tokio::test]
async fn capability_permission_profiles_leave_full_the_whole_tool_set_unattended() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.permissions = PermissionProfile::Full;
    req.env.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;

    let args = std::fs::read_to_string(&args_file).unwrap();
    let argv: Vec<&str> = args.lines().collect();
    // Full leaves the whole tool set available: no --tools restriction.
    assert!(
        !argv.contains(&"--tools"),
        "Full must not restrict the tools: {argv:?}"
    );
    // acceptEdits is the one mode that runs unattended as root (permissions.rs).
    let mode_pos = argv
        .iter()
        .position(|a| *a == "--permission-mode")
        .expect("every profile sets an unattended permission mode");
    assert_eq!(argv[mode_pos + 1], "acceptEdits");
    // Both are refused when the CLI runs as root — confirmed empirically
    // (see permissions.rs) — so neither may appear in the built args.
    assert!(
        !args.contains("--dangerously-skip-permissions"),
        "got: {args}"
    );
    assert!(!args.contains("bypassPermissions"), "got: {args}");
}

#[tokio::test]
async fn capability_custom_agents_passes_the_agent_as_its_own_flag() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.model = Some("claude-opus-5".into());
    req.agent = Some("benito".into());
    req.env.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
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
async fn capability_resume_session_passes_the_session_id_to_the_resume_flag() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
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
    let child_pid_file = child_pid_fifo(dir.path());

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CLAUDE_STUB_CHILD_PID_FILE".to_string(),
        child_pid_file.display().to_string().into(),
    );
    req.env
        .insert("CLAUDE_STUB_HANG".to_string(), "1".to_string().into());
    let mut session = adapter().spawn(req).await.unwrap();
    let grandchild_pid = grandchild_pid(&child_pid_file).await;

    session.kill().await.unwrap();

    // The grandchild the stub spawned must die too, not just the
    // stub itself — proving the kill reached the whole process group.
    assert!(
        stops_running(&grandchild_pid).await,
        "grandchild process survived kill()"
    );
}

/// The fifo the stub records its child pid into. A fifo, not a plain file,
/// so [`grandchild_pid`] blocks on it and wakes the instant the stub writes,
/// a rendezvous with the child rather than a poll of the filesystem.
fn child_pid_fifo(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("child.pid");
    let status = std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .expect("mkfifo runs");
    assert!(status.success(), "mkfifo creates the child-pid fifo");
    path
}

/// The pid of the blocking child the stub spawned. The path is a fifo, so the
/// read blocks until the stub opens it and writes the pid: an explicit
/// rendezvous with the child, not a timed poll. The deadline turns a stub that
/// never records the pid into a failed test rather than a hung one.
async fn grandchild_pid(child_pid_fifo: &std::path::Path) -> String {
    let fifo = child_pid_fifo.to_path_buf();
    tokio::time::timeout(
        Duration::from_secs(30),
        tokio::task::spawn_blocking(move || std::fs::read_to_string(fifo)),
    )
    .await
    .expect("the stub records the child pid before the deadline")
    .expect("the pid reader joins")
    .expect("the child-pid fifo reads")
    .trim()
    .to_string()
}

/// True while `pid` runs. `kill -0` alone is not enough: a killed
/// process whose parent is gone lingers as a zombie — still visible to
/// `kill -0` — until something reaps it, so running means `ps` reports
/// a state for it that is not `Z`.
fn running(pid: &str) -> bool {
    let ps = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", pid])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&ps.stdout);
    let state = state.trim();
    !(state.is_empty() || state.starts_with('Z'))
}

/// Polls until `pid` stops running; `false` when it never does.
async fn stops_running(pid: &str) -> bool {
    for _ in 0..50 {
        if !running(pid) {
            return true;
        }
        tokio::task::yield_now().await;
    }
    false
}

#[tokio::test]
async fn dropping_a_session_kills_its_process_tree() {
    let dir = tempfile::tempdir().unwrap();
    let child_pid_file = child_pid_fifo(dir.path());

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CLAUDE_STUB_CHILD_PID_FILE".to_string(),
        child_pid_file.display().to_string().into(),
    );
    req.env
        .insert("CLAUDE_STUB_HANG".to_string(), "1".to_string().into());
    let session = adapter().spawn(req).await.unwrap();
    let grandchild_pid = grandchild_pid(&child_pid_file).await;

    // No kill(), no interrupt(): the session goes out of scope — what
    // an early return or a panic in the engine looks like from here.
    drop(session);

    assert!(
        stops_running(&grandchild_pid).await,
        "the grandchild survived the session being dropped"
    );
}

#[tokio::test]
async fn non_utf8_output_does_not_end_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let lines = dir.path().join("lines.jsonl");
    let mut scripted = Vec::new();
    scripted.extend_from_slice(INIT_LINE.as_bytes());
    scripted.push(b'\n');
    // A note whose text carries bytes no UTF-8 decoder accepts.
    scripted.extend_from_slice(
        br#"{"type":"assistant","message":{"id":"msg-1","content":[{"type":"text","text":"caf"#,
    );
    scripted.extend_from_slice(b"\xff\xfe");
    scripted.extend_from_slice(br#""}]}}"#);
    scripted.push(b'\n');
    scripted.extend_from_slice(br#"{"type":"result","is_error":false,"result":"all done"}"#);
    scripted.push(b'\n');
    std::fs::write(&lines, scripted).unwrap();

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(matches!(events[0], AgentEvent::SessionOpened { .. }));
    assert!(
        matches!(
            events.last(),
            Some(AgentEvent::Completed { result }) if result.summary == "all done"
        ),
        "the session must reach its terminal event past the undecodable line, got: {events:?}"
    );
}

#[tokio::test]
async fn prompt_travels_by_stdin_never_argv() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let stdin_file = dir.path().join("stdin.txt");
    write_lines(dir.path(), ".claude-stub-lines.jsonl", &[]);
    let mut req = request(dir.path().to_path_buf());
    req.prompt = "the whole brief, with a --flag-looking line".to_string();
    req.env.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.to_str().unwrap().to_string().into(),
    );
    req.env.insert(
        "CLAUDE_STUB_STDIN_FILE".to_string(),
        stdin_file.to_str().unwrap().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    drain(session).await;

    let args = std::fs::read_to_string(&args_file).unwrap();
    assert!(
        !args.contains("the whole brief"),
        "the prompt must never be an argument (visible in `ps`): {args}"
    );
    let stdin = std::fs::read_to_string(&stdin_file).unwrap();
    assert_eq!(stdin, "the whole brief, with a --flag-looking line");
}

#[test]
fn debug_of_a_session_request_never_prints_secrets() {
    let mut req = request(std::path::PathBuf::from("/tmp"));
    req.env
        .insert("API_TOKEN".to_string(), "hunter2".to_string().into());
    req.run_tools_endpoint = Some(yunta_adapters::RunToolsEndpoint {
        url: "http://127.0.0.1:1/mcp".to_string(),
        token: "bearer-secret".to_string().into(),
    });
    let debug = format!("{req:?}");
    assert!(
        debug.contains("API_TOKEN"),
        "the name stays visible: {debug}"
    );
    assert!(
        !debug.contains("hunter2"),
        "the value never prints: {debug}"
    );
    assert!(
        !debug.contains("bearer-secret"),
        "the token never prints: {debug}"
    );
    assert_eq!(
        debug.matches("[redacted]").count(),
        2,
        "both the env value and the endpoint token are redacted: {debug}"
    );
}

/// `Edit` is its own tool set — file editing, no shell, no network —
/// which is what makes `permission_profiles` more than a name.
#[tokio::test]
async fn capability_permission_profiles_give_edit_a_bounded_tool_set() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.permissions = PermissionProfile::Edit;
    req.env.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;

    let args: Vec<String> = std::fs::read_to_string(&args_file)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    let pos = args
        .iter()
        .position(|a| a == "--tools")
        .expect("Edit restricts the tools");
    // The exact set: the file-editing tools plus read-only navigation,
    // and nothing that reaches a shell (Bash) or the network (WebFetch,
    // WebSearch) — the bound the profile's name promises.
    assert_eq!(
        args[pos + 1],
        "Read,Grep,Glob,Edit,Write,MultiEdit,NotebookEdit"
    );
}

/// `skills` is backed by staging every resolved skill directory into
/// the CLI's own discovery location under the session's cwd.
#[tokio::test]
async fn capability_skills_are_staged_into_the_clis_discovery_directory() {
    let dir = tempfile::tempdir().unwrap();
    let skill = dir.path().join("skills-src/grill");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), "# grill\n").unwrap();
    let cwd = dir.path().join("worktree");
    std::fs::create_dir_all(&cwd).unwrap();
    write_lines(&cwd, ".claude-stub-lines.jsonl", &[]);

    let mut req = request(cwd.clone());
    req.skills = vec![skill.clone()];
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;

    let staged = cwd.join(".claude/skills/grill");
    assert!(
        std::fs::symlink_metadata(&staged).is_ok_and(|m| m.file_type().is_symlink()),
        "the skill is staged as a link under .claude/skills"
    );
    assert_eq!(std::fs::read_link(&staged).unwrap(), skill);
}

#[tokio::test]
async fn budget_max_turns_reaches_the_cli_as_a_flag() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.budget.max_turns = Some(7);
    req.env.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;

    let args: Vec<String> = std::fs::read_to_string(&args_file)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    let pos = args
        .iter()
        .position(|a| a == "--max-turns")
        .expect("--max-turns is passed");
    assert_eq!(args[pos + 1], "7");
}

#[tokio::test]
async fn an_unknown_adapter_setting_is_reported_by_probe() {
    let mut extra = serde_json::Map::new();
    extra.insert("max_thinking".to_string(), serde_json::Value::from(3));
    let adapter = ClaudeCodeAdapter::new(&AdapterSettings {
        adapter_settings: Some(extra),
        binary: Some(stub_path()),
    });
    let err = adapter.probe().await.unwrap_err();
    let text = err.to_string();
    assert!(
        text.contains("max_thinking") && text.contains("adapter_settings"),
        "{text}"
    );
}

// --- a strict reading of the CLI's protocol ---------------------------

/// Runs a session over `lines` scripted for the stub and returns every
/// event it produced.
async fn events_of(lines: &[&str]) -> Vec<AgentEvent> {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(dir.path(), "lines.jsonl", lines);
    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    drain(session).await
}

fn non_retryable_failure(event: &AgentEvent) -> &str {
    match event {
        AgentEvent::Failed { error, retryable } => {
            assert!(!retryable, "not a failure to retry: {}", error.message);
            &error.message
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[tokio::test]
async fn init_without_session_id_fails_non_retryable() {
    let events = events_of(&[
        r#"{"type":"system","subtype":"init","model":"claude-sonnet-5"}"#,
        r#"{"type":"result","is_error":false,"result":"all done"}"#,
    ])
    .await;

    let message = non_retryable_failure(&events[0]);
    assert!(
        message.contains("session_id"),
        "the failure names the missing field: {message}"
    );
    assert_eq!(
        events.len(),
        1,
        "nothing follows a session's terminal event: {events:?}"
    );
}

#[tokio::test]
async fn auth_errors_are_not_retryable() {
    let events = events_of(&[
        INIT_LINE,
        r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"Invalid API key · Please run /login"}"#,
    ])
    .await;

    let message = non_retryable_failure(events.last().unwrap());
    assert_eq!(message, "Invalid API key · Please run /login");
}

#[tokio::test]
async fn an_invalid_model_is_not_retried() {
    let events = events_of(&[
        INIT_LINE,
        r#"{"type":"result","is_error":true,"result":"API Error: 404 {\"type\":\"error\",\"error\":{\"type\":\"not_found_error\",\"message\":\"model: claude-nope\"}}"}"#,
    ])
    .await;

    non_retryable_failure(events.last().unwrap());
}

#[tokio::test]
async fn a_result_line_without_is_error_is_a_non_retryable_failure() {
    let events = events_of(&[INIT_LINE, r#"{"type":"result","result":"all done"}"#]).await;

    let message = non_retryable_failure(events.last().unwrap());
    assert!(
        message.contains("is_error"),
        "the failure names the missing field: {message}"
    );
}

#[tokio::test]
async fn an_event_before_the_session_opens_fails_the_session() {
    let events = events_of(&[
        r#"{"type":"assistant","message":{"id":"msg-0","content":[{"type":"text","text":"hi"}]}}"#,
        INIT_LINE,
        r#"{"type":"result","is_error":false,"result":"all done"}"#,
    ])
    .await;

    let message = non_retryable_failure(&events[0]);
    assert!(
        message.contains("before"),
        "the failure says what came before the session opened: {message}"
    );
    assert_eq!(events.len(), 1, "got: {events:?}");
}

/// Helper: spawn with the stub, return the argv it was launched with.
async fn argv_for(dir: &std::path::Path, mut req: SessionRequest) -> Vec<String> {
    let args_file = dir.join("args.txt");
    let lines = write_lines(dir, "lines.jsonl", &[]);
    req.env.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    req.env.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;
    std::fs::read_to_string(&args_file)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn a_declared_artifact_directory_is_writable_by_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("run/artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();

    let mut req = request(dir.path().to_path_buf());
    req.artifact_dir = Some(artifacts.clone());
    let args = argv_for(dir.path(), req).await;

    // The CLI confines writes to its working directory, and the run's
    // artifact directory is never inside it: without this the agent is
    // told to write a file it is then refused permission to create.
    let pos = args
        .iter()
        .position(|a| a == "--add-dir")
        .expect("a declared artifact directory is added to the writable set");
    assert_eq!(args[pos + 1], artifacts.display().to_string());
}

#[tokio::test]
async fn no_declared_artifact_widens_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let args = argv_for(dir.path(), request(dir.path().to_path_buf())).await;
    assert!(
        !args.iter().any(|a| a == "--add-dir"),
        "a node that declares no artifact needs no widening: {args:?}"
    );
}

#[tokio::test]
async fn read_only_still_writes_its_own_declared_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path().to_path_buf());
    req.permissions = PermissionProfile::ReadOnly;
    let args = argv_for(dir.path(), req).await;

    // `read-only` means the node does not touch the project. Its own
    // declared artifact is the node's output, not the project.
    let pos = args.iter().position(|a| a == "--tools").unwrap();
    assert!(
        args[pos + 1].split(',').any(|t| t == "Write"),
        "read-only must still be able to produce what it declares: {}",
        args[pos + 1]
    );
}

#[tokio::test]
async fn the_per_run_tools_reach_the_session_without_the_token_on_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().join("run/scratch");
    std::fs::create_dir_all(&scratch).unwrap();

    let mut req = request(dir.path().to_path_buf());
    req.scratch_dir = Some(scratch.clone());
    req.run_tools_endpoint = Some(yunta_adapters::RunToolsEndpoint {
        url: "http://127.0.0.1:54321/mcp".to_string(),
        token: "s3cr3t-token-value".to_string().into(),
    });
    let args = argv_for(dir.path(), req).await;

    let pos = args
        .iter()
        .position(|a| a == "--mcp-config")
        .expect("the per-run MCP server is mounted");
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&args[pos + 1]).unwrap()).unwrap();
    let server = &config["mcpServers"]["yunta"];
    assert_eq!(server["url"], "http://127.0.0.1:54321/mcp");
    assert_eq!(
        server["headers"]["Authorization"], "Bearer s3cr3t-token-value",
        "the listener authenticates every call"
    );

    // The token is secret material: argv is world-readable through `ps`.
    assert!(
        !args.iter().any(|a| a.contains("s3cr3t-token-value")),
        "the token must never reach the process list: {args:?}"
    );
    // Mounting a server the profile then forbids would be a tool the
    // agent is told to call and cannot.
    assert!(
        args.iter().any(|a| a == "mcp__yunta"),
        "the mounted tools are allowed — by server, so this adapter \
         never has to know which tools the engine mounts: {args:?}"
    );
}

#[tokio::test]
async fn no_per_run_endpoint_mounts_no_server() {
    let dir = tempfile::tempdir().unwrap();
    let args = argv_for(dir.path(), request(dir.path().to_path_buf())).await;
    assert!(
        !args.iter().any(|a| a == "--mcp-config"),
        "nothing to mount, nothing mounted: {args:?}"
    );
}
