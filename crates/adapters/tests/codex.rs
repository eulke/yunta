//! Integration tests for the real `codex` adapter against a fake
//! `codex` binary (`fixtures/codex_stub.sh`) — no network, no API cost,
//! no real LLM in CI. What this suite cannot cover — whether the
//! real CLI's actual output matches what the stub scripts — has no
//! manual smoke test to fall back on either: no `codex` binary or
//! credentials exist in this environment. See `codex/mod.rs`'s own doc
//! comment for the details.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use yunta_adapters::{
    Adapter, AgentEvent, Budget, CodexAdapter, PermissionProfile, ProbeReport, SessionRequest,
};
use yunta_core::{AdapterSettings, SessionId};

fn stub_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex_stub.sh")
}

fn adapter() -> CodexAdapter {
    CodexAdapter::new(&AdapterSettings {
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

const THREAD_STARTED_LINE: &str = r#"{"type":"thread.started","thread_id":"thread-abc"}"#;

#[tokio::test]
async fn probe_reports_the_stub_as_healthy_with_its_version() {
    let report = adapter().probe().await.unwrap();
    let ProbeReport::Healthy { version } = report else {
        panic!("the stub is healthy: {report:?}");
    };
    assert!(version.unwrap().contains("0.47.0"));
}

#[tokio::test]
async fn capabilities_declare_what_this_adapter_actually_does() {
    let caps = adapter().capabilities();
    assert!(caps.resume_session);
    assert!(caps.permission_profiles);
    assert!(caps.usage_reporting);
    // Never claim a capability that isn't wired end-to-end yet.
    assert!(!caps.custom_agents);
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
            THREAD_STARTED_LINE,
            r#"{"type":"turn.started"}"#,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"all done"}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":4}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.model = Some("gpt-5-codex".into());
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(matches!(
        &events[0],
        AgentEvent::SessionOpened { model: None, .. }
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
        lines.display().to_string().into(),
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
        lines.display().to_string().into(),
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
        lines.display().to_string().into(),
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
        lines.display().to_string().into(),
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
        lines.display().to_string().into(),
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
        lines.display().to_string().into(),
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
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], AgentEvent::SessionOpened { .. }));
}

#[tokio::test]
async fn a_session_reports_no_model_because_the_cli_names_none() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(dir.path(), "lines.jsonl", &[THREAD_STARTED_LINE]);

    let mut req = request(dir.path().to_path_buf());
    req.model = Some("gpt-5-codex".parse().unwrap());
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    // The request named a model; the CLI's `thread.started` never
    // says which one runs, and the session reports only what the CLI
    // said.
    assert!(matches!(
        &events[0],
        AgentEvent::SessionOpened { model: None, .. }
    ));
}

#[tokio::test]
async fn capability_permission_profiles_map_to_their_own_sandbox_modes() {
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
            args_file.display().to_string().into(),
        );
        req.env.insert(
            "CODEX_STUB_LINES_FILE".to_string(),
            lines.display().to_string().into(),
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
    req.model = Some("gpt-5-codex".into());
    req.env.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
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
    assert_eq!(args[model_pos + 1], "gpt-5-codex");
}

#[tokio::test]
async fn capability_resume_session_passes_the_thread_id_to_the_resume_subcommand() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
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

/// `codex exec resume` takes the thread id, `--last`, `--all` and
/// `--image`; `exec`'s own options are declared on the parent command
/// and are not `global`, so clap reads one that follows the subcommand
/// as an unexpected argument and the invocation dies before a session
/// opens.
#[tokio::test]
async fn resume_places_every_parent_option_before_the_subcommand() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("run/artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[THREAD_STARTED_LINE]);

    let mut req = request(dir.path().to_path_buf());
    req.model = Some("gpt-5-codex".into());
    req.permissions = PermissionProfile::Edit;
    req.artifact_dir = Some(artifacts);
    req.env.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
    );
    let session = adapter()
        .resume(&SessionId::from("thread-to-resume"), req)
        .await
        .unwrap();
    let events = drain(session).await;

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::Failed { .. })),
        "the CLI accepts the invocation: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::SessionOpened { .. })),
        "the resumed session opens: {events:?}"
    );

    let args: Vec<String> = std::fs::read_to_string(&args_file)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    let position = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .unwrap_or_else(|| panic!("`{flag}` reaches the CLI: {args:?}"))
    };
    let resume_pos = position("resume");
    assert!(
        resume_pos > position("--model"),
        "the model is the parent command's option: {args:?}"
    );
    assert!(
        resume_pos > position("--sandbox"),
        "the sandbox is the parent command's option: {args:?}"
    );
    for (at, arg) in args.iter().enumerate() {
        assert!(
            arg != "-c" || resume_pos > at,
            "every config override precedes the subcommand: {args:?}"
        );
    }
    assert_eq!(
        args[resume_pos + 1..].to_vec(),
        vec!["thread-to-resume".to_string(), "-".to_string()],
        "the subcommand takes the thread id and the stdin prompt, nothing else: {args:?}"
    );
}

/// The stub stands in for the CLI's own parser, so what clap rejects it
/// rejects too: a stub that accepted any argument order would let an
/// invocation the real binary refuses pass the suite.
#[test]
fn the_stub_refuses_a_parent_option_after_resume_like_clap_does() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(dir.path(), "lines.jsonl", &[THREAD_STARTED_LINE]);
    let run = |args: [&str; 7]| {
        std::process::Command::new(stub_path())
            .args(args)
            .env("CODEX_STUB_LINES_FILE", &lines)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };

    let refused = run(["exec", "--json", "resume", "x", "--model", "m", "-"]);
    assert_eq!(refused.status.code(), Some(2), "the parse fails");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("unexpected argument"),
        "the diagnostic names the argument: {}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(
        refused.stdout.is_empty(),
        "no session opens: {}",
        String::from_utf8_lossy(&refused.stdout)
    );

    let accepted = run(["exec", "--json", "--model", "m", "resume", "x", "-"]);
    assert_eq!(accepted.status.code(), Some(0), "the parse succeeds");
    assert!(
        String::from_utf8_lossy(&accepted.stdout).contains("thread.started"),
        "the session streams its events: {}",
        String::from_utf8_lossy(&accepted.stdout)
    );
}

#[tokio::test]
async fn kill_terminates_the_whole_process_tree_including_grandchildren() {
    let dir = tempfile::tempdir().unwrap();
    let child_pid_file = child_pid_fifo(dir.path());

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_CHILD_PID_FILE".to_string(),
        child_pid_file.display().to_string().into(),
    );
    req.env
        .insert("CODEX_STUB_HANG".to_string(), "1".to_string().into());
    let mut session = adapter().spawn(req).await.unwrap();

    let grandchild_pid = grandchild_pid(&child_pid_file).await;

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
        tokio::task::yield_now().await;
    }
    assert!(!grandchild_running, "grandchild process survived kill()");
}

/// The fifo the stub records its child pid into. A fifo, not a plain file, so
/// [`grandchild_pid`] blocks on it and wakes the instant the stub writes, a
/// rendezvous with the child rather than a poll of the filesystem.
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

#[tokio::test]
async fn prompt_travels_by_stdin_never_argv() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let stdin_file = dir.path().join("stdin.txt");
    write_lines(dir.path(), ".codex-stub-lines.jsonl", &[]);
    let mut req = request(dir.path().to_path_buf());
    req.prompt = "the whole brief, with a --flag-looking line".to_string();
    req.env.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.to_str().unwrap().to_string().into(),
    );
    req.env.insert(
        "CODEX_STUB_STDIN_FILE".to_string(),
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

/// `adapter_settings.sandbox` is the mode the `Edit` profile runs
/// under; the other two profiles keep their own modes.
#[tokio::test]
async fn the_sandbox_setting_governs_the_edit_profile_only() {
    let mut extra = serde_json::Map::new();
    extra.insert(
        "sandbox".to_string(),
        serde_json::Value::from("danger-full-access"),
    );
    let adapter = CodexAdapter::new(&AdapterSettings {
        adapter_settings: Some(extra),
        binary: Some(stub_path()),
    });
    for (profile, expected) in [
        (PermissionProfile::ReadOnly, "read-only"),
        (PermissionProfile::Edit, "danger-full-access"),
        (PermissionProfile::Full, "danger-full-access"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let args_file = dir.path().join("args.txt");
        let lines = write_lines(dir.path(), "lines.jsonl", &[]);
        let mut req = request(dir.path().to_path_buf());
        req.permissions = profile;
        req.env.insert(
            "CODEX_STUB_ARGS_FILE".to_string(),
            args_file.display().to_string().into(),
        );
        req.env.insert(
            "CODEX_STUB_LINES_FILE".to_string(),
            lines.display().to_string().into(),
        );
        let session = adapter.spawn(req).await.unwrap();
        let _ = drain(session).await;
        let args: Vec<String> = std::fs::read_to_string(&args_file)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        let pos = args.iter().position(|a| a == "--sandbox").unwrap();
        assert_eq!(args[pos + 1], expected, "{profile:?}");
    }
}

#[tokio::test]
async fn an_unknown_adapter_setting_is_reported_by_probe() {
    let mut extra = serde_json::Map::new();
    extra.insert("sandbx".to_string(), serde_json::Value::from("read-only"));
    let adapter = CodexAdapter::new(&AdapterSettings {
        adapter_settings: Some(extra),
        binary: Some(stub_path()),
    });
    let err = adapter.probe().await.unwrap_err();
    let text = err.to_string();
    assert!(
        text.contains("sandbx") && text.contains("sandbox"),
        "the unknown key and the known ones are named: {text}"
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
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    drain(session).await
}

#[tokio::test]
async fn auth_errors_are_not_retryable() {
    let events = events_of(&[
        THREAD_STARTED_LINE,
        r#"{"type":"turn.failed","error":{"message":"401 Unauthorized: invalid API key"}}"#,
    ])
    .await;

    match events.last().unwrap() {
        AgentEvent::Failed { error, retryable } => {
            assert!(!retryable, "not a failure to retry: {}", error.message);
            assert!(error.message.contains("401"));
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[tokio::test]
async fn a_fatal_error_event_ends_the_session_as_failed() {
    let events = events_of(&[
        THREAD_STARTED_LINE,
        r#"{"type":"error","message":"stream disconnected before completion"}"#,
    ])
    .await;

    match events.last().unwrap() {
        AgentEvent::Failed { error, retryable } => {
            assert!(retryable, "a transport failure is one to retry");
            assert_eq!(error.message, "stream disconnected before completion");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[tokio::test]
async fn a_declared_artifact_directory_is_writable_by_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("run/artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let args_file = dir.path().join("args.txt");

    let mut req = request(dir.path().to_path_buf());
    req.artifact_dir = Some(artifacts.clone());
    req.env.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;
    let args = std::fs::read_to_string(&args_file).unwrap();

    // `workspace-write` confines writes to the workspace, and the run's
    // artifact directory is never inside it. The expectation spells the
    // TOML out rather than deriving it the way the adapter does, so a
    // wrong rendering cannot agree with itself.
    let expected = format!(
        "sandbox_workspace_write.writable_roots=[\"{}/run/artifacts\"]",
        dir.path().display()
    );
    assert!(
        args.lines().any(|arg| arg == expected),
        "the artifact directory joins the writable roots as {expected}: {args}"
    );
}

/// A path is arbitrary bytes; a TOML string is not. A directory whose
/// name carries the characters that end one reaches the CLI as a single
/// value that reads back whole, not as three broken tokens. Which
/// quoting carries it — basic or literal — is the renderer's call, so
/// the claim here is what the CLI parses, never how it was spelled.
#[tokio::test]
async fn an_artifact_directory_with_toml_metacharacters_reaches_the_cli_whole() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join(r#"quote"and\slash"#);
    std::fs::create_dir_all(&artifacts).unwrap();
    let args_file = dir.path().join("args.txt");

    let expected_dir = artifacts.clone();
    let mut req = request(dir.path().to_path_buf());
    req.artifact_dir = Some(artifacts);
    req.env.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;
    let args = std::fs::read_to_string(&args_file).unwrap();

    let assignment = args
        .lines()
        .find(|arg| arg.starts_with("sandbox_workspace_write.writable_roots="))
        .unwrap_or_else(|| panic!("the writable roots reach the CLI: {args}"));
    let table: toml::Table = toml::from_str(assignment)
        .unwrap_or_else(|e| panic!("`{assignment}` is not readable TOML: {e}"));
    assert_eq!(
        table["sandbox_workspace_write"]["writable_roots"],
        toml::Value::Array(vec![toml::Value::String(
            expected_dir.display().to_string()
        )]),
        "the path the CLI reads is the path it was given: {assignment}"
    );
}

#[tokio::test]
async fn the_per_run_tools_reach_the_session_with_the_token_only_in_the_environment() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let env_file = dir.path().join("env.txt");

    let mut req = request(dir.path().to_path_buf());
    req.run_tools_endpoint = Some(yunta_adapters::RunToolsEndpoint {
        url: "http://127.0.0.1:54321/mcp".to_string(),
        token: "s3cr3t-token-value".to_string().into(),
    });
    req.env.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    req.env.insert(
        "CODEX_STUB_ENV_FILE".to_string(),
        env_file.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;
    let args = std::fs::read_to_string(&args_file).unwrap();
    let env = std::fs::read_to_string(&env_file).unwrap();

    assert!(
        args.contains("mcp_servers.yunta.url=\"http://127.0.0.1:54321/mcp\""),
        "the per-run server is configured: {args}"
    );
    // The CLI reads the credential from a named variable rather than
    // from its own config, which is what keeps it out of argv.
    assert!(
        args.contains("mcp_servers.yunta.bearer_token_env_var=\"YUNTA_RUN_TOOLS_TOKEN\""),
        "the credential is named, not inlined: {args}"
    );
    assert!(
        env.contains("YUNTA_RUN_TOOLS_TOKEN=s3cr3t-token-value"),
        "the token reaches the child by environment"
    );
    assert!(
        !args.contains("s3cr3t-token-value"),
        "the token must never reach the process list: {args}"
    );
}

#[tokio::test]
async fn no_per_run_endpoint_configures_no_server() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;
    assert!(
        !std::fs::read_to_string(&args_file)
            .unwrap()
            .contains("mcp_servers"),
        "nothing to configure, nothing configured"
    );
}

/// The CLI reads a server as streamable HTTP from the `url` key alone.
/// Every override this adapter sends is one the CLI's configuration
/// reference defines, so a key the CLI does not know cannot ride along
/// and silently do nothing.
#[tokio::test]
async fn no_dead_config_override_reaches_the_cli() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");

    let mut req = request(dir.path().to_path_buf());
    req.run_tools_endpoint = Some(yunta_adapters::RunToolsEndpoint {
        url: "http://127.0.0.1:54321/mcp".to_string(),
        token: "s3cr3t-token-value".to_string().into(),
    });
    req.env.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;

    let args: Vec<String> = std::fs::read_to_string(&args_file)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert!(
        !args
            .iter()
            .any(|a| a.contains("experimental_use_rmcp_client")),
        "no key outside the CLI's configuration reference: {args:?}"
    );
    let server: Vec<&String> = args
        .iter()
        .filter(|a| a.starts_with("mcp_servers.yunta."))
        .collect();
    assert_eq!(
        server.len(),
        2,
        "the per-run server takes its url and its credential's variable, nothing more: {args:?}"
    );
    assert!(
        server
            .iter()
            .any(|a| a.starts_with("mcp_servers.yunta.url=")),
        "the url selects streamable HTTP: {args:?}"
    );
    assert!(
        server
            .iter()
            .any(|a| a.starts_with("mcp_servers.yunta.bearer_token_env_var=")),
        "the credential is named, not inlined: {args:?}"
    );
}
