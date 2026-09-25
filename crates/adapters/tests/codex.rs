//! Integration tests for the real `codex` adapter against a fake
//! `codex` binary (`yunta_testkit_core::stubs::codex`) — no network, no
//! API cost, no real LLM in CI. What this suite cannot cover — whether the
//! real CLI's actual output matches what the stub scripts — has no
//! manual smoke test to fall back on either: no `codex` binary or
//! credentials exist in this environment. See `codex/mod.rs`'s own doc
//! comment for the details.

use std::error::Error as _;
use std::path::PathBuf;
use yunta_core::fence::{Advice, Fence};

use yunta_adapters::CodexAdapter;
use yunta_core::events::SessionEnd;
use yunta_core::port::{Adapter, AgentEvent, PermissionProfile, ProbeReport};
use yunta_core::Pid;
use yunta_core::{AdapterSettings, SessionId};
use yunta_testkit_core::adapter::{
    child_pid_fifo, drain, drain_for_exit, grandchild_pid, request, wait_until_gone, write_lines,
};

fn stub_path() -> PathBuf {
    yunta_testkit_core::stubs::codex()
}

fn adapter() -> CodexAdapter {
    CodexAdapter::new(&AdapterSettings {
        adapter_settings: None,
        binary: Some(stub_path()),
    })
}

const THREAD_STARTED_LINE: &str = r#"{"type":"thread.started","thread_id":"thread-abc"}"#;

const TURN_COMPLETED_LINE: &str =
    r#"{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}"#;

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
    // The sandbox the process runs under confines writes by directory.
    assert_eq!(caps.fence, yunta_core::FenceLevel::Filesystem);
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
            input_tokens: Some(10),
            output_tokens: Some(4),
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
async fn a_command_execution_item_maps_to_tool_use_digesting_its_command() {
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
        AgentEvent::ToolUse { name, target }
            if name == "command_execution"
                && target.display.is_none()
                && target.digest == yunta_core::sha256_hex(b"cargo test")
    )));
}

#[tokio::test]
async fn a_file_change_item_maps_to_tool_use_digesting_its_first_path() {
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
        AgentEvent::ToolUse { name, target }
            if name == "file_change"
                && target.display.as_deref() == Some("src/lib.rs")
    )));
}

#[tokio::test]
async fn an_mcp_tool_call_item_maps_to_tool_use_digesting_its_server_and_tool() {
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
        AgentEvent::ToolUse { name, target }
            if name == "mcp_tool_call"
                && target.display.as_deref() == Some("yunta:query")
    )));
}

#[tokio::test]
async fn a_web_search_item_maps_to_tool_use_digesting_its_query() {
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
        AgentEvent::ToolUse { name, target }
            if name == "web_search"
                && target.display.is_none()
                && target.digest == yunta_core::sha256_hex(b"codex exec json schema")
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

/// A CLI that refuses its configuration says so on stderr and exits
/// before opening anything. That is the whole of what the engine has to
/// go on, so the session hands it over rather than dropping it into a
/// trace nobody reads.
#[tokio::test]
async fn a_session_that_dies_before_its_first_event_reports_its_exit_and_its_last_stderr_lines() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path().to_path_buf());
    let said = write_lines(
        dir.path(),
        "stderr.txt",
        &["url is not supported for stdio"],
    );
    req.env.insert(
        "CODEX_STUB_STDERR_FILE".to_string(),
        said.display().to_string().into(),
    );
    req.env
        .insert("CODEX_STUB_EXIT".to_string(), "2".to_string().into());
    let (events, exit) = drain_for_exit(adapter().spawn(req).await.unwrap())
        .await
        .unwrap();

    assert!(events.is_empty(), "the CLI said nothing: {events:?}");
    let exit = exit.expect("a session with a process of its own says how it ended");
    assert_eq!(exit.end, SessionEnd::Code { code: 2 });
    assert_eq!(exit.stderr_tail, ["url is not supported for stdio"]);
}

/// The child's environment is where this system puts its secrets, and a
/// CLI that fails at startup is exactly the one liable to echo what it
/// was handed straight back.
#[tokio::test]
async fn a_dead_sessions_stderr_tail_never_carries_a_value_from_its_env() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "YUNTA_RUN_TOOLS_TOKEN".to_string(),
        "s3cr3t-token-value".to_string().into(),
    );
    let said = write_lines(
        dir.path(),
        "stderr.txt",
        &["auth failed for bearer s3cr3t-token-value"],
    );
    req.env.insert(
        "CODEX_STUB_STDERR_FILE".to_string(),
        said.display().to_string().into(),
    );
    req.env
        .insert("CODEX_STUB_EXIT".to_string(), "1".to_string().into());
    let (_, exit) = drain_for_exit(adapter().spawn(req).await.unwrap())
        .await
        .unwrap();

    let tail = exit
        .expect("the session ended with a process of its own")
        .stderr_tail;
    assert_eq!(tail, ["auth failed for bearer [redacted]"], "{tail:?}");
}

/// Asking is killing first: nothing of a session outlives the run, and
/// the wait for its status is bounded by a process already dead.
#[tokio::test]
async fn a_dead_sessions_process_group_is_gone_once_its_exit_is_collected() {
    let dir = tempfile::tempdir().unwrap();
    let child_pid_file = child_pid_fifo(dir.path());
    let lines = write_lines(dir.path(), "lines.jsonl", &[THREAD_STARTED_LINE]);

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_CHILD_PID_FILE".to_string(),
        child_pid_file.display().to_string().into(),
    );
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
    );
    req.env
        .insert("CODEX_STUB_HANG".to_string(), "1".to_string().into());
    let mut session = adapter().spawn(req).await.unwrap();
    let grandchild: i32 = grandchild_pid(&child_pid_file).await.parse().unwrap();

    assert!(
        session.exit().await.unwrap().is_some(),
        "the session had a process"
    );
    wait_until_gone(Pid::try_from(grandchild).unwrap()).await;
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
    req.fence = Fence::everything(vec![artifacts], Advice::ReportFinding);
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
    let req = request(dir.path().to_path_buf());
    events_with(&dir, req, lines).await
}

/// The same, for a request a test shaped itself.
async fn events_with(
    dir: &tempfile::TempDir,
    mut req: yunta_core::port::SessionRequest,
    lines: &[&str],
) -> Vec<AgentEvent> {
    let lines = write_lines(dir.path(), "lines.jsonl", lines);
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
async fn a_codex_session_keeps_the_fence_roots_writable_beside_the_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("run/artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let args_file = dir.path().join("args.txt");

    let mut req = request(dir.path().to_path_buf());
    req.fence = Fence::everything(vec![artifacts.clone()], Advice::ReportFinding);
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
        "a fence root joins the writable roots as {expected}: {args}"
    );
}

/// A path is arbitrary bytes; a TOML string is not. A directory whose
/// name carries the characters that end one reaches the CLI as a single
/// value that reads back whole, not as three broken tokens. Which
/// quoting carries it — basic or literal — is the renderer's call, so
/// the claim here is what the CLI parses, never how it was spelled.
#[tokio::test]
async fn a_writable_root_with_toml_metacharacters_reaches_the_cli_whole() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join(r#"quote"and\slash"#);
    std::fs::create_dir_all(&artifacts).unwrap();
    let args_file = dir.path().join("args.txt");

    let expected_dir = artifacts.clone();
    let mut req = request(dir.path().to_path_buf());
    req.fence = Fence::everything(vec![artifacts], Advice::ReportFinding);
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
    req.run_tools_endpoint = Some(yunta_core::port::RunToolsEndpoint {
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
        args.contains("mcp_servers.yunta-run.url=\"http://127.0.0.1:54321/mcp\""),
        "the per-run server is configured: {args}"
    );
    // The CLI reads the credential from a named variable rather than
    // from its own config, which is what keeps it out of argv.
    assert!(
        args.contains("mcp_servers.yunta-run.bearer_token_env_var=\"YUNTA_RUN_TOOLS_TOKEN\""),
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

/// A person registers the control plane under whatever name they like,
/// and the name they reach for is this system's own. The per-run server
/// carries a name of its own so a CLI that merges both by key never
/// reads one entry as the other.
#[tokio::test]
async fn the_per_run_server_never_shares_the_control_planes_name() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");

    let mut req = request(dir.path().to_path_buf());
    req.run_tools_endpoint = Some(yunta_core::port::RunToolsEndpoint {
        url: "http://127.0.0.1:54321/mcp".to_string(),
        token: "s3cr3t-token-value".to_string().into(),
    });
    req.env.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;
    let args = std::fs::read_to_string(&args_file).unwrap();

    assert!(
        args.contains("mcp_servers.yunta-run.url="),
        "the per-run server has a name of its own: {args}"
    );
    assert!(
        !args.contains("mcp_servers.yunta."),
        "and never the one a person's own entry takes: {args}"
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
    req.run_tools_endpoint = Some(yunta_core::port::RunToolsEndpoint {
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
        .filter(|a| a.starts_with("mcp_servers.yunta-run."))
        .collect();
    assert_eq!(
        server.len(),
        2,
        "the per-run server takes its url and its credential's variable, nothing more: {args:?}"
    );
    assert!(
        server
            .iter()
            .any(|a| a.starts_with("mcp_servers.yunta-run.url=")),
        "the url selects streamable HTTP: {args:?}"
    );
    assert!(
        server
            .iter()
            .any(|a| a.starts_with("mcp_servers.yunta-run.bearer_token_env_var=")),
        "the credential is named, not inlined: {args:?}"
    );
}

#[tokio::test]
async fn a_tool_use_never_persists_the_command_it_ran() {
    let dir = tempfile::tempdir().unwrap();
    let command = "psql postgres://admin:hunter2@db.internal/prod -c 'select 1'";
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            THREAD_STARTED_LINE,
            &format!(
                r#"{{"type":"item.completed","item":{{"id":"item_0","type":"command_execution","command":"{command}","aggregated_output":"ok","exit_code":0,"status":"completed"}}}}"#
            ),
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

    let targets: Vec<&yunta_core::events::ToolTarget> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolUse { target, .. } => Some(target),
            _ => None,
        })
        .collect();
    assert_eq!(targets.len(), 1, "the stream carries the one call it made");
    assert_eq!(
        targets[0].display, None,
        "a command is the session's own text: identified, never shown"
    );
    let carried = format!("{:?}", targets[0]);
    for fragment in ["psql", "hunter2", "db.internal", "select"] {
        assert!(
            !carried.contains(fragment),
            "`{fragment}` of the command reached the log as `{carried}`"
        );
    }
    assert_eq!(
        targets[0].digest,
        yunta_core::sha256_hex(command.as_bytes())
    );
}

/// A count the CLI did not report is absent, never zero. A run that
/// recorded zero would say the session cost nothing, which is a
/// different claim from "the CLI said nothing about it".
#[tokio::test]
async fn a_missing_token_count_is_absent_not_zero() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            THREAD_STARTED_LINE,
            // A `usage` naming only what it counted.
            r#"{"type":"turn.completed","usage":{"input_tokens":40}}"#,
        ],
    );

    let mut req = request(dir.path().to_path_buf());
    req.env.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string().into(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    let usage = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::Usage {
                input_tokens,
                output_tokens,
                cached_input_tokens,
            } => Some((*input_tokens, *output_tokens, *cached_input_tokens)),
            _ => None,
        })
        .expect("the turn reported its usage");
    assert_eq!(usage, (Some(40), None, None));
}

/// A line kind this adapter does not read is tolerated: the stream goes
/// on, and the line contributes nothing rather than failing the session
/// or being mistaken for something it is not.
#[tokio::test]
async fn an_unknown_stream_line_is_tolerated_and_named() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            THREAD_STARTED_LINE,
            // A kind the CLI could add tomorrow, and one it already has
            // that this adapter deliberately ignores.
            r#"{"type":"thread.renamed","name":"something new"}"#,
            r#"{"type":"turn.started"}"#,
            r#"{"type":"item.completed","item":{"id":"i0","type":"agent_message","text":"still here"}}"#,
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

    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::Note { text } if text == "still here")),
        "the stream went on past the line it does not read: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::Completed { .. })),
        "and the session still reached its terminal: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::Failed { .. })),
        "an unread line is not a failure: {events:?}"
    );
}

/// Settings that do not read fail the session rather than fall back to
/// a default: a `sandbox:` nobody could parse would run the agent under
/// a confinement the team never asked for.
#[tokio::test]
async fn a_session_never_opens_under_settings_that_do_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let settings = yunta_core::AdapterSettings {
        adapter_settings: Some(
            serde_json::from_str(r#"{"sandbox": "no-such-mode"}"#).expect("the settings parse"),
        ),
        ..Default::default()
    };
    let adapter = yunta_adapters::CodexAdapter::new(&settings);

    let Err(error) = adapter.spawn(request(dir.path().to_path_buf())).await else {
        panic!("a session must not open under settings nobody could read");
    };
    let text = yunta_core::describe(&error);
    assert!(
        text.contains("cannot read its `adapter_settings`") && text.contains("no-such-mode"),
        "the refusal names what it could not read: {text}"
    );
}

/// A thread id the CLI states and nothing can be fails the session, and
/// the failure keeps what rejected the value so a reader following the
/// chain reaches the rule it broke.
#[tokio::test]
async fn a_thread_id_that_cannot_be_one_fails_keeping_what_rejected_it() {
    let events = events_of(&[r#"{"type":"thread.started","thread_id":""}"#]).await;

    let AgentEvent::Failed { error, retryable } = &events[0] else {
        panic!("expected Failed, got {:?}", events[0]);
    };
    assert!(!retryable, "not a failure to retry: {error}");
    let described = yunta_core::describe(error);
    assert!(
        described.starts_with("the CLI's `thread.started` line is malformed"),
        "the failure names the line: {described}"
    );
    assert!(
        error.source().is_some(),
        "the failure keeps what rejected the value: {described}"
    );
    assert!(
        described.len() > error.message.len(),
        "the cause is read, not dropped: {described}"
    );
}

/// The sandbox is by directory in both channels, so the coverage a
/// session reports is the widened one, naming the directories.
#[tokio::test]
async fn a_codex_session_reports_widened_coverage_with_its_roots() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("run/artifacts");
    let mut req = request(dir.path().to_path_buf());
    req.fence = Fence::everything(vec![artifacts.clone()], Advice::ReportFinding);

    let events = events_with(&dir, req, &[THREAD_STARTED_LINE, TURN_COMPLETED_LINE]).await;

    let AgentEvent::SessionOpened { fence, .. } = &events[0] else {
        panic!("a session opens first: {events:?}");
    };
    assert_eq!(
        fence.as_ref(),
        Some(&yunta_core::fence::Coverage::WidenedToRoots {
            roots: vec![dir.path().to_path_buf(), artifacts],
        })
    );
}

/// The sandbox has one setting for the whole filesystem: `read-only`
/// seals the roots along with everything else, so a session that must
/// write a declared file could never produce one. A capability that is
/// absent fails instead of spending the session.
#[tokio::test]
async fn a_read_only_codex_session_with_declared_files_fails_before_spawning() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path().to_path_buf());
    req.permissions = PermissionProfile::ReadOnly;
    req.fence = Fence::read_only(
        vec![dir.path().join("run/artifacts")],
        Advice::ReportFinding,
    );

    let refused = adapter()
        .spawn(req)
        .await
        .err()
        .expect("a fence this sandbox cannot build fails before spawning");
    let said = yunta_core::describe(&refused);
    assert!(
        said.contains("read-only profile"),
        "the refusal names what it cannot keep writable: {said}"
    );
}

/// A command the sandbox refused is a write that did not happen, not
/// activity to chronicle as a tool call.
#[tokio::test]
async fn a_sandbox_denial_becomes_write_refused() {
    let denied = r#"{"type":"item.completed","item":{"type":"command_execution","command":"tee /etc/hosts","status":"sandbox_denied"}}"#;
    let events = events_of(&[THREAD_STARTED_LINE, denied, TURN_COMPLETED_LINE]).await;

    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::WriteRefused { .. })),
        "the denial is recorded as a refused write: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::ToolUse { .. })),
        "and never also as a tool call: {events:?}"
    );
}
