//! Integration tests for the real `codex` adapter against a fake
//! `codex` binary (`yunta_testkit_core::stubs::codex`) — no network, no
//! API cost, no real LLM in CI. What this suite cannot cover — whether the
//! real CLI's actual output matches what the stub scripts — has no
//! check run by hand to fall back on either: no `codex` binary or
//! credentials exist in this environment. See `codex/mod.rs`'s own doc
//! comment for the details.

use std::error::Error as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::StreamExt;
use yunta_core::fence::{Advice, Fence};

use yunta_adapters::CodexAdapter;
use yunta_core::events::SessionEnd;
use yunta_core::port::{Adapter, AgentEvent, PermissionProfile};
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
async fn a_run_tool_is_named_the_way_this_cli_hands_it_to_the_model() {
    // A session's own tool list names the server's tools with its `-`
    // written `_`: `mcp__yunta_run__yunta_check_artifact` is what a
    // session found when it looked `yunta_check_artifact` up.
    let naming = adapter().capabilities().tool_naming;
    assert_eq!(
        yunta_core::RunTool::CheckArtifact.called(naming),
        "mcp__yunta_run__yunta_check_artifact"
    );
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
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    match events.last().unwrap() {
        AgentEvent::Failed { error, cause } => {
            assert_eq!(error.message, "model response stream ended unexpectedly");
            assert!(cause.retryable());
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
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::ToolUse { name, target }
            if name == "command_execution"
                && target.display().is_none()
                && *target.digest() == yunta_core::sha256_hex(b"cargo test")
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
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::ToolUse { name, target }
            if name == "file_change"
                && target.display() == Some("src/lib.rs")
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
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::ToolUse { name, target }
            if name == "mcp_tool_call"
                && target.display() == Some("yunta:query")
    )));
}

#[tokio::test]
async fn failed_run_tool_calls_report_only_a_known_tool_and_closed_cause() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(
        dir.path(),
        "lines.jsonl",
        &[
            THREAD_STARTED_LINE,
            r#"{"type":"item.completed","item":{"type":"mcp_tool_call","server":"yunta-run","tool":"yunta_submit_questions","status":"failed","error":{"message":"approval policy is Never; secret=top-secret"},"arguments":{"secret":"top-secret"}}}"#,
            r#"{"type":"item.completed","item":{"type":"mcp_tool_call","server":"yunta-run","tool":"yunta_submit_questions","status":"failed","arguments":{"secret":"top-secret"}}}"#,
            r#"{"type":"item.completed","item":{"type":"mcp_tool_call","server":"yunta-run","tool":"yunta_submit_questions","status":"approval_denied","arguments":{"secret":"top-secret"}}}"#,
            r#"{"type":"item.completed","item":{"type":"mcp_tool_call","server":"other","tool":"yunta_submit_questions","status":"failed","error":"approval blocked"}}"#,
            r#"{"type":"item.completed","item":{"type":"mcp_tool_call","server":"yunta-run","tool":"unknown_tool","status":"failed","error":"approval blocked"}}"#,
        ],
    );
    let mut req = request(dir.path().to_path_buf());
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let events = drain(adapter().spawn(req).await.unwrap()).await;
    let failed: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::RunToolFailed { tool, cause } => Some((*tool, *cause)),
            _ => None,
        })
        .collect();
    assert_eq!(
        failed,
        vec![
            (
                yunta_core::RunTool::Submit(yunta_core::ArtifactKind::Questions),
                yunta_core::events::RunToolFailureCause::CallFailed
            ),
            (
                yunta_core::RunTool::Submit(yunta_core::ArtifactKind::Questions),
                yunta_core::events::RunToolFailureCause::CallFailed
            ),
            (
                yunta_core::RunTool::Submit(yunta_core::ArtifactKind::Questions),
                yunta_core::events::RunToolFailureCause::ApprovalBlocked
            ),
        ]
    );
    assert!(!format!("{failed:?}").contains("top-secret"));
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
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::ToolUse { name, target }
            if name == "web_search"
                && target.display().is_none()
                && *target.digest() == yunta_core::sha256_hex(b"codex exec json schema")
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
    req.env.plain.insert(
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
    req.env.plain.insert(
        "CODEX_STUB_STDERR_FILE".to_string(),
        said.display().to_string(),
    );
    req.env
        .plain
        .insert("CODEX_STUB_EXIT".to_string(), "2".to_string());
    let (events, exit) = drain_for_exit(adapter().spawn(req).await.unwrap())
        .await
        .unwrap();

    assert!(events.is_empty(), "the CLI said nothing: {events:?}");
    let exit = exit.expect("a session with a process of its own says how it ended");
    assert_eq!(exit.end, SessionEnd::Code { code: 2 });
    assert_eq!(exit.stderr_tail, ["url is not supported for stdio"]);
}

/// Asking is killing first: nothing of a session outlives the run, and
/// the wait for its status is bounded by a process already dead.
#[tokio::test]
async fn a_dead_sessions_process_group_is_gone_once_its_exit_is_collected() {
    let dir = tempfile::tempdir().unwrap();
    let child_pid_file = child_pid_fifo(dir.path());
    let lines = write_lines(dir.path(), "lines.jsonl", &[THREAD_STARTED_LINE]);

    let mut req = request(dir.path().to_path_buf());
    req.env.plain.insert(
        "CODEX_STUB_CHILD_PID_FILE".to_string(),
        child_pid_file.display().to_string(),
    );
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    req.env
        .plain
        .insert("CODEX_STUB_HANG".to_string(), "1".to_string());
    let mut session = adapter().spawn(req).await.unwrap();
    let grandchild: i32 = grandchild_pid(&child_pid_file).await.parse().unwrap();

    assert!(
        session.exit().await.unwrap().is_some(),
        "the session had a process"
    );
    wait_until_gone(Pid::try_from(grandchild).unwrap()).await;
}

/// A CLI that keeps running after it has printed its last line leaves its
/// stream open: the session waits for the CLI, not for the lines.
#[tokio::test]
async fn a_lingering_codex_stub_keeps_its_stream_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path().to_path_buf());
    req.env
        .plain
        .insert("CODEX_STUB_LINGER".to_string(), "1".to_string());
    let mut session = adapter().spawn(req).await.unwrap();
    let next = tokio::time::timeout(Duration::from_millis(500), session.events().next()).await;
    assert!(
        next.is_err(),
        "the stub is still running, so its stream is open"
    );
}

/// A named agent's instructions reach the CLI as the key a profile
/// carries them in, added to its own.
#[tokio::test]
async fn a_named_agent_s_instructions_reach_the_cli_as_developer_instructions() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);
    let mut req = request(dir.path().to_path_buf());
    req.agent = Some(yunta_core::agent::AgentDefinition {
        name: "benito".into(),
        instructions: "Review like \"Benito\".".to_string(),
    });
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let _ = drain(adapter().spawn(req).await.unwrap()).await;

    let args = tokio::fs::read_to_string(&args_file).await.unwrap();
    let assignment = args
        .lines()
        .find(|arg| arg.starts_with("developer_instructions="))
        .expect("the instructions are passed");
    let table: toml::Table = toml::from_str(assignment).unwrap();
    assert_eq!(
        table["developer_instructions"].as_str(),
        Some("Review like \"Benito\".")
    );
}

/// A skill source and a worktree, both created under `dir`.
fn skill_and_cwd(dir: &Path) -> (PathBuf, PathBuf) {
    let (skill, cwd) = (dir.join("skills-src/grill"), dir.join("worktree"));
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::create_dir_all(&cwd).unwrap();
    (skill, cwd)
}

/// A skill is mounted where the CLI discovers a project's skills, declared
/// as staged, and taken back once the session is released.
#[tokio::test]
async fn a_skill_is_mounted_where_the_cli_finds_it_and_taken_back_after() {
    let dir = tempfile::tempdir().unwrap();
    let (skill, cwd) = skill_and_cwd(dir.path());
    let mut req = request(cwd.clone());
    req.skills = vec![skill.clone()];
    let staged = [std::path::PathBuf::from(".agents/skills/grill")];
    assert_eq!(adapter().staged_paths(&req), staged);

    let lines = write_lines(dir.path(), "lines.jsonl", &[THREAD_STARTED_LINE]);
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    // The session holds its mount until it is released, so the link is read
    // while the session is still alive.
    let mut session = adapter().spawn(req).await.unwrap();
    let events: Vec<AgentEvent> = session.events().collect().await;
    assert!(!events.is_empty());
    let link = tokio::fs::read_link(cwd.join(&staged[0])).await.unwrap();
    assert_eq!(link, skill);

    session.release().await.unwrap();
    assert!(!tokio::fs::try_exists(cwd.join(".agents")).await.unwrap());
}

/// A launch refused for want of its hook mounts nothing: the refusal comes
/// before any skill is linked.
#[tokio::test]
async fn a_codex_launch_refused_for_its_hook_stages_no_skill() {
    let dir = tempfile::tempdir().unwrap();
    let (skill, cwd) = skill_and_cwd(dir.path());
    let mut req = request(cwd.clone());
    req.skills = vec![skill];
    req.rail_command = None;
    assert!(adapter().spawn(req).await.is_err());
    assert!(
        !cwd.join(".agents").exists(),
        "a refused launch leaves no skill link"
    );
}

/// A session whose CLI cannot be started takes back the skill it mounted.
#[tokio::test]
async fn a_session_that_fails_to_open_leaves_no_skill_link() {
    let dir = tempfile::tempdir().unwrap();
    let (skill, cwd) = skill_and_cwd(dir.path());
    let missing = CodexAdapter::new(&AdapterSettings {
        adapter_settings: None,
        binary: Some("/nonexistent/codex".into()),
    });
    let mut req = request(cwd.clone());
    req.skills = vec![skill];
    assert!(missing.spawn(req).await.is_err());
    assert!(std::fs::symlink_metadata(cwd.join(".agents/skills/grill")).is_err());
}

/// The CLI's `thread.started` names no model, so the session reports none,
/// whatever model it was asked to run.
#[tokio::test]
async fn a_codex_session_reports_no_model_from_its_stream() {
    let dir = tempfile::tempdir().unwrap();
    let lines = write_lines(dir.path(), "lines.jsonl", &[THREAD_STARTED_LINE]);

    let mut req = request(dir.path().to_path_buf());
    req.model = Some("gpt-5-codex".parse().unwrap());
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(
        matches!(&events[0], AgentEvent::SessionOpened { model: None, .. }),
        "{events:?}"
    );
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
        req.env.plain.insert(
            "CODEX_STUB_ARGS_FILE".to_string(),
            args_file.display().to_string(),
        );
        req.env.plain.insert(
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
    req.model = Some("gpt-5-codex".into());
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.plain.insert(
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
async fn capability_resume_session_passes_the_thread_id_to_the_resume_subcommand() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    req.effort = Some(yunta_core::Effort::Xhigh);
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
    let effort = args
        .iter()
        .position(|a| a == r#"model_reasoning_effort="xhigh""#)
        .expect("a resumed session keeps its effort");
    assert!(
        effort < resume_pos,
        "an `exec` option precedes the subcommand"
    );
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
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
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
        let args = argv_under(&adapter, profile).await;
        let pos = args.iter().position(|a| a == "--sandbox").unwrap();
        assert_eq!(args[pos + 1], expected, "{profile:?}");
    }
}

/// The argv `adapter` launches the CLI with for a session under
/// `profile`.
async fn argv_under(adapter: &CodexAdapter, profile: PermissionProfile) -> Vec<String> {
    argv_with(adapter, profile, yunta_core::CliConfig::Inherit).await
}

/// The same, for a session that loads `cli_config`.
async fn argv_with(
    adapter: &CodexAdapter,
    profile: PermissionProfile,
    cli_config: yunta_core::CliConfig,
) -> Vec<String> {
    argv_of(adapter, profile, cli_config, true).await
}

/// The same, for a session whose commands may reach the network or not.
async fn argv_of(
    adapter: &CodexAdapter,
    profile: PermissionProfile,
    cli_config: yunta_core::CliConfig,
    network: bool,
) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);
    let mut req = request(dir.path().to_path_buf());
    req.permissions = profile;
    req.cli_config = cli_config;
    req.network = network;
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter.spawn(req).await.unwrap();
    let _ = drain(session).await;
    std::fs::read_to_string(&args_file)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

/// An isolated session loads none of the person's own configuration or
/// rules; an inheriting one loads them as the CLI always does.
#[tokio::test]
async fn an_isolated_session_ignores_the_person_s_own_config_and_rules() {
    let adapter = CodexAdapter::new(&AdapterSettings {
        adapter_settings: None,
        binary: Some(stub_path()),
    });
    let isolated = argv_with(
        &adapter,
        PermissionProfile::Edit,
        yunta_core::CliConfig::Isolated,
    )
    .await;
    assert!(
        isolated.contains(&"--ignore-user-config".to_string()),
        "{isolated:?}"
    );
    assert!(
        isolated.contains(&"--ignore-rules".to_string()),
        "{isolated:?}"
    );

    let inherited = argv_under(&adapter, PermissionProfile::Edit).await;
    assert!(
        !inherited.iter().any(|arg| arg.starts_with("--ignore-")),
        "{inherited:?}"
    );
}

/// A session's request says whether its commands reach the network; only
/// `workspace-write` reads it — `read-only` has none to open and
/// `danger-full-access` never closed it.
#[tokio::test]
async fn the_request_opens_the_network_only_where_workspace_write_closed_it() {
    const OPENS: &str = "sandbox_workspace_write.network_access=true";
    for (network, settings, profile, opened) in [
        (true, serde_json::json!({}), PermissionProfile::Edit, true),
        (false, serde_json::json!({}), PermissionProfile::Edit, false),
        (
            true,
            serde_json::json!({}),
            PermissionProfile::ReadOnly,
            false,
        ),
        (true, serde_json::json!({}), PermissionProfile::Full, false),
        (
            true,
            serde_json::json!({ "sandbox": "danger-full-access" }),
            PermissionProfile::Edit,
            false,
        ),
    ] {
        let serde_json::Value::Object(extra) = settings.clone() else {
            unreachable!()
        };
        let adapter = CodexAdapter::new(&AdapterSettings {
            adapter_settings: Some(extra),
            binary: Some(stub_path()),
        });
        let args = argv_of(&adapter, profile, yunta_core::CliConfig::Inherit, network).await;
        assert_eq!(
            args.iter().any(|arg| arg == OPENS),
            opened,
            "network {network}, {settings} under {profile:?}: {args:?}"
        );
    }
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
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    drain(session).await
}

#[tokio::test]
async fn a_fatal_error_event_ends_the_session_as_failed() {
    let events = events_of(&[
        THREAD_STARTED_LINE,
        r#"{"type":"error","message":"stream disconnected before completion"}"#,
    ])
    .await;

    match events.last().unwrap() {
        AgentEvent::Failed { error, cause } => {
            assert!(cause.retryable(), "a transport failure is one to retry");
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
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
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
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
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
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.plain.insert(
        "CODEX_STUB_ENV_FILE".to_string(),
        env_file.display().to_string(),
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
        args.contains("mcp_servers.yunta-run.default_tools_approval_mode=\"approve\""),
        "calls on the ephemeral server are approved without prompting: {args}"
    );
    assert!(
        args.contains("mcp_servers.yunta-run.tool_timeout_sec=3600.0"),
        "a call that runs criteria is waited for as long as a session with no wall clock \
         may run: {args}"
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
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
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
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
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
    req.env.plain.insert(
        "CODEX_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
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
        4,
        "the per-run server takes its url, credential variable, approval mode and call \
         timeout: {args:?}"
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

/// A thread id the CLI states and nothing can be fails the session, and
/// the failure keeps what rejected the value so a reader following the
/// chain reaches the rule it broke.
#[tokio::test]
async fn a_thread_id_that_cannot_be_one_fails_keeping_what_rejected_it() {
    let events = events_of(&[r#"{"type":"thread.started","thread_id":""}"#]).await;

    let AgentEvent::Failed { error, cause } = &events[0] else {
        panic!("expected Failed, got {:?}", events[0]);
    };
    assert!(!cause.retryable(), "not a failure to retry: {error}");
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

/// Edits are judged exactly; commands are confined by the sandbox's
/// directories, so the coverage a session reports is the widened one,
/// naming them.
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

/// `read-only` seals the run's roots along with everything else, so a
/// read-only session that must write a declared file runs under
/// `workspace-write`: its edits are refused by the hook everywhere but
/// the roots, and its commands are confined to the workspace and the
/// roots, which its coverage says.
#[tokio::test]
async fn a_read_only_codex_session_with_declared_files_writes_them_under_workspace_write() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("run/artifacts");
    let mut req = request(dir.path().to_path_buf());
    req.permissions = PermissionProfile::ReadOnly;
    req.fence = Fence::read_only(vec![artifacts.clone()], Advice::ReportFinding);

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

/// A read-only sandbox from the settings seals the roots along with the rest,
/// so the commands of an edit session cover no root and the session reports
/// exact coverage.
#[tokio::test]
async fn a_read_only_sandbox_from_the_settings_leaves_the_commands_no_root() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("run/artifacts");
    let mut extra = serde_json::Map::new();
    extra.insert("sandbox".to_string(), serde_json::Value::from("read-only"));
    let adapter = CodexAdapter::new(&AdapterSettings {
        adapter_settings: Some(extra),
        binary: Some(stub_path()),
    });
    let mut req = request(dir.path().to_path_buf());
    req.fence = Fence::everything(vec![artifacts], Advice::ReportFinding);
    let lines = write_lines(dir.path(), "lines.jsonl", &[THREAD_STARTED_LINE]);
    req.env.plain.insert(
        "CODEX_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let events = drain(adapter.spawn(req).await.unwrap()).await;

    let AgentEvent::SessionOpened { fence, .. } = &events[0] else {
        panic!("a session opens first: {events:?}");
    };
    assert_eq!(fence.as_ref(), Some(&yunta_core::fence::Coverage::Exact));
}

/// The hook reaches the CLI as an override it trusts in the same call:
/// no hook of anyone else's runs untrusted to make room for it.
#[tokio::test]
async fn a_codex_session_installs_its_write_hook_trusted_and_nothing_more() {
    let args = argv_under(&adapter(), PermissionProfile::Edit).await;
    let assigned = |key: &str| {
        args.iter()
            .find(|arg| arg.starts_with(&format!("{key}=")))
            .cloned()
            .unwrap_or_default()
    };
    assert!(
        assigned("hooks.PreToolUse").contains("rail codex before-write"),
        "{args:?}"
    );
    assert!(
        assigned("hooks.state").contains("pre_tool_use:0:0"),
        "{args:?}"
    );
    assert!(
        !args.iter().any(|arg| arg.contains("bypass-hook-trust")),
        "{args:?}"
    );
}

/// A sandbox refusal is an ordinary command to the adapter: it comes out as
/// the tool use it is.
#[tokio::test]
async fn a_sandbox_denial_is_not_a_refused_write() {
    let denied = r#"{"type":"item.completed","item":{"type":"command_execution","command":"tee /etc/hosts","status":"sandbox_denied"}}"#;
    let events = events_of(&[THREAD_STARTED_LINE, denied, TURN_COMPLETED_LINE]).await;
    assert!(
        events.iter().any(|event| {
            matches!(event, AgentEvent::ToolUse { name, .. } if name == "command_execution")
        }),
        "the denied command comes out as a tool use: {events:?}"
    );
}
