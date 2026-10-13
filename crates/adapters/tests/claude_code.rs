//! Integration tests for the real `claude-code` adapter against a fake
//! `claude` binary (`yunta_testkit_core::stubs::claude_code`) — no network, no
//! API cost, no real LLM in CI. The one thing this suite cannot cover
//! is whether the real CLI's actual output matches what the stub
//! scripts: a check run by hand against the real CLI covers that instead.
//!
//! Stub configuration travels through `SessionRequest.env.plain` (the
//! child process's own environment), never by mutating the process
//! environment in a test — cargo runs tests concurrently in one process,
//! and a process-global env var would race across them. A credential
//! travels through `env.secret`, whose values the stderr tail redacts.

use std::error::Error as _;
use std::path::{Path, PathBuf};
use std::time::Duration;
use yunta_core::fence::{Advice, Fence};

use futures::StreamExt;
use yunta_adapters::ClaudeCodeAdapter;
use yunta_core::port::{Adapter, AgentEvent, PermissionProfile, SessionRequest};
use yunta_core::{AdapterSettings, SessionId};
use yunta_testkit_core::adapter::{child_pid_fifo, drain, grandchild_pid, request, write_lines};

fn stub_path() -> PathBuf {
    yunta_testkit_core::stubs::claude_code()
}

fn adapter() -> ClaudeCodeAdapter {
    ClaudeCodeAdapter::new(&AdapterSettings {
        adapter_settings: None,
        binary: Some(stub_path()),
    })
}

const INIT_LINE: &str =
    r#"{"type":"system","subtype":"init","session_id":"sess-abc","model":"claude-sonnet-5"}"#;

const RESULT_LINE: &str = r#"{"type":"result","is_error":false,"result":"all done"}"#;

#[tokio::test]
async fn a_run_tool_is_named_the_way_this_cli_lists_it_in_its_init_line() {
    // The init line of a session holding the run tools lists each one as
    // the model calls it: that name, and nothing shorter, reaches it.
    let naming = adapter().capabilities().tool_naming;
    assert_eq!(
        yunta_core::RunTool::Task.called(naming),
        "mcp__yunta-run__yunta_task"
    );
    assert_eq!(
        yunta_core::RunTool::from_called("mcp__yunta-run__yunta_task", naming),
        Some(yunta_core::RunTool::Task)
    );
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
    req.env.plain.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    match events.last().unwrap() {
        AgentEvent::Failed { error, cause } => {
            assert_eq!(error.message, "rate limited");
            assert!(cause.retryable());
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[tokio::test]
async fn a_tool_use_block_maps_to_tool_use_digesting_its_target() {
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
    req.env.plain.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let events = drain(session).await;

    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::ToolUse { name, target }
            if name == "Edit" && target.display() == Some("src/lib.rs")
    )));
}

#[tokio::test]
async fn capability_permission_profiles_give_read_only_the_non_mutating_tools() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.permissions = PermissionProfile::ReadOnly;
    req.env.plain.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.plain.insert(
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
    let tools_pos = args
        .iter()
        .position(|a| a == "--tools")
        .expect("ReadOnly restricts the tools");
    assert_eq!(
        args[tools_pos + 1],
        "Read,Grep,Glob,WebFetch,WebSearch,Bash"
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
    req.env.plain.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.plain.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
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

/// A named agent's instructions join the CLI's own system prompt, read
/// from a file in the session's scratch — never in place of the prompt,
/// and beside the model the runner chose.
#[tokio::test]
async fn capability_custom_agents_appends_the_agent_s_instructions() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path().to_path_buf());
    req.model = Some("claude-opus-5".into());
    req.agent = Some(yunta_core::agent::AgentDefinition {
        name: "benito".into(),
        instructions: "Review like Benito.".to_string(),
    });
    let scratch = req.scratch_dir.clone();
    let args = argv_for(dir.path(), req).await;

    let model_pos = args.iter().position(|a| a == "--model").unwrap();
    assert_eq!(args[model_pos + 1], "claude-opus-5");
    assert!(!args
        .iter()
        .any(|a| a == "--agent" || a == "--system-prompt"));
    let file_pos = args
        .iter()
        .position(|a| a == "--append-system-prompt-file")
        .expect("the instructions are appended");
    assert_eq!(
        args[file_pos + 1],
        scratch.join("agent.md").display().to_string()
    );
    assert_eq!(
        std::fs::read_to_string(scratch.join("agent.md")).unwrap(),
        "Review like Benito."
    );
}

#[tokio::test]
async fn capability_resume_session_passes_the_session_id_to_the_resume_flag() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.env.plain.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.plain.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    req.effort = Some(yunta_core::Effort::Xhigh);
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
    let effort_pos = args.iter().position(|a| a == "--effort").unwrap();
    assert_eq!(
        args[effort_pos + 1],
        "xhigh",
        "a resumed session keeps its effort"
    );
}

/// One that ignores the interrupt is given the whole grace, and no more.
#[tokio::test]
async fn an_interrupted_session_that_stays_is_given_the_whole_grace() {
    let dir = tempfile::tempdir().unwrap();
    let child_pid_file = child_pid_fifo(dir.path());
    let mut req = request(dir.path().to_path_buf());
    req.env.plain.insert(
        "CLAUDE_STUB_CHILD_PID_FILE".to_string(),
        child_pid_file.display().to_string(),
    );
    req.env
        .plain
        .insert("CLAUDE_STUB_HANG".to_string(), "1".to_string());
    let mut session = adapter().spawn(req).await.unwrap();
    grandchild_pid(&child_pid_file).await;

    session.interrupt().await.unwrap();
    let started = std::time::Instant::now();
    session.allow_exit(Duration::from_millis(300)).await;

    let waited = started.elapsed();
    session.kill().await.unwrap();
    assert!(waited >= Duration::from_millis(300), "waited {waited:?}");
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
    req.env.plain.insert(
        "CLAUDE_STUB_CHILD_PID_FILE".to_string(),
        child_pid_file.display().to_string(),
    );
    req.env
        .plain
        .insert("CLAUDE_STUB_HANG".to_string(), "1".to_string());
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
    req.env.plain.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
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

/// `Edit` is its own tool set — file editing, no shell, no network —
/// which is what makes `permission_profiles` more than a name.
#[tokio::test]
async fn capability_permission_profiles_give_edit_a_bounded_tool_set() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = dir.path().join("args.txt");
    let lines = write_lines(dir.path(), "lines.jsonl", &[]);

    let mut req = request(dir.path().to_path_buf());
    req.permissions = PermissionProfile::Edit;
    req.env.plain.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.plain.insert(
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
    let pos = args
        .iter()
        .position(|a| a == "--tools")
        .expect("Edit restricts the tools");
    // The exact set: the file-editing tools, read-only navigation and a
    // shell the sandbox confines, and no web tools — the bound the
    // profile's name promises.
    assert_eq!(
        args[pos + 1],
        "Read,Grep,Glob,Bash,Edit,Write,MultiEdit,NotebookEdit"
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
    // The link belongs to the session: it is there while the session lives,
    // and the session takes it away when it is dropped.
    let staged = cwd.join(".claude/skills/grill");
    assert!(
        std::fs::symlink_metadata(&staged).is_ok_and(|m| m.file_type().is_symlink()),
        "the skill is staged as a link under .claude/skills"
    );
    assert_eq!(std::fs::read_link(&staged).unwrap(), skill);
    let _ = drain(session).await;
}

/// What the adapter staged is its own mechanics, never the agent's work,
/// so it leaves the session's checkout once the session ends — and with
/// it the discovery directories it created, when nothing else is there.
#[tokio::test]
async fn a_skill_mount_is_gone_once_its_session_ends() {
    let dir = tempfile::tempdir().unwrap();
    let skill = dir.path().join("skills-src/grill");
    tokio::fs::create_dir_all(&skill).await.unwrap();
    tokio::fs::write(skill.join("SKILL.md"), "# grill\n")
        .await
        .unwrap();
    let cwd = dir.path().join("worktree");
    tokio::fs::create_dir_all(&cwd).await.unwrap();
    write_lines(&cwd, ".claude-stub-lines.jsonl", &[]);

    let mut req = request(cwd.clone());
    req.skills = vec![skill.clone()];
    let mut session = adapter().spawn(req).await.unwrap();
    let _events: Vec<AgentEvent> = session.events().collect().await;
    session.release().await.unwrap();

    assert!(
        tokio::fs::symlink_metadata(cwd.join(".claude/skills/grill"))
            .await
            .is_err(),
        "the link is gone"
    );
    assert!(
        !cwd.join(".claude").exists(),
        "and so are the empty directories"
    );
    assert!(
        skill.join("SKILL.md").exists(),
        "the skill itself is untouched"
    );
}

/// A skill source and a worktree, both created under `dir`.
fn skill_and_cwd(dir: &Path) -> (PathBuf, PathBuf) {
    let (skill, cwd) = (dir.join("skills-src/grill"), dir.join("worktree"));
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::create_dir_all(&cwd).unwrap();
    (skill, cwd)
}

/// A launch whose CLI cannot be started takes back the skill links it mounted.
#[tokio::test]
async fn a_session_that_fails_to_open_leaves_no_skill_link() {
    let dir = tempfile::tempdir().unwrap();
    let (skill, cwd) = skill_and_cwd(dir.path());
    let missing = ClaudeCodeAdapter::new(&AdapterSettings {
        adapter_settings: None,
        binary: Some("/nonexistent/claude".into()),
    });
    let mut req = request(cwd.clone());
    req.skills = vec![skill];
    assert!(missing.spawn(req).await.is_err());
    assert!(std::fs::symlink_metadata(cwd.join(".claude/skills/grill")).is_err());
}

/// A launch that fails while it writes the agent's instructions mounts no skill.
#[tokio::test]
async fn a_claude_launch_that_fails_while_writing_its_agent_leaves_no_skill_link() {
    let dir = tempfile::tempdir().unwrap();
    let (skill, cwd) = skill_and_cwd(dir.path());
    let reviewer: yunta_core::AgentName = "reviewer".parse().unwrap();
    let mut req = request(cwd.clone());
    req.skills = vec![skill];
    req.agent = Some(
        yunta_core::agent::AgentDefinition::parse(
            &reviewer,
            "---\nname: reviewer\ndescription: d\n---\nWork.\n",
        )
        .unwrap(),
    );
    req.scratch_dir = dir.path().join("scratch-is-a-file");
    std::fs::write(&req.scratch_dir, "x").unwrap();
    assert!(adapter().spawn(req).await.is_err());
    assert!(std::fs::symlink_metadata(cwd.join(".claude/skills/grill")).is_err());
}

// --- a strict reading of the CLI's protocol ---------------------------

/// Runs a session over `lines` scripted for the stub and returns every
/// event it produced.
async fn events_of(lines: &[&str]) -> Vec<AgentEvent> {
    let dir = tempfile::tempdir().unwrap();
    let req = request(dir.path().to_path_buf());
    spawn_with(&dir, req, lines).await
}

/// The same, for a request a test shaped itself.
async fn spawn_with(
    dir: &tempfile::TempDir,
    mut req: SessionRequest,
    lines: &[&str],
) -> Vec<AgentEvent> {
    let lines = write_lines(dir.path(), "lines.jsonl", lines);
    req.env.plain.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    drain(session).await
}

fn non_retryable_failure(event: &AgentEvent) -> &str {
    match event {
        AgentEvent::Failed { error, cause } => {
            assert!(
                !cause.retryable(),
                "not a failure to retry: {}",
                error.message
            );
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
    req.env.plain.insert(
        "CLAUDE_STUB_ARGS_FILE".to_string(),
        args_file.display().to_string(),
    );
    req.env.plain.insert(
        "CLAUDE_STUB_LINES_FILE".to_string(),
        lines.display().to_string(),
    );
    let session = adapter().spawn(req).await.unwrap();
    let _ = drain(session).await;
    std::fs::read_to_string(&args_file)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

/// Everything the fence installs, and nothing of it under the worktree:
/// the engine as a `PreToolUse` hook, and the roots the CLI would
/// otherwise refuse.
#[tokio::test]
async fn a_claude_session_installs_the_fence_by_settings_and_add_dir_and_nothing_under_cwd() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("run/artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();

    let mut req = request(dir.path().to_path_buf());
    req.fence = Fence {
        allowed: Some(vec!["src/**".into()]),
        roots: vec![artifacts.clone()],
        advice: Advice::ReportFinding,
        protected: yunta_core::protected::Protected::default(),
        naming: yunta_core::ToolNaming::Bare,
    };
    let args = argv_for(dir.path(), req).await;

    // The CLI confines writes to its working directory, and a root the
    // fence keeps writable is never inside it: without this the agent
    // is told to write a file it is then refused permission to create.
    let pos = args
        .iter()
        .position(|a| a == "--add-dir")
        .expect("every fence root is added to the writable set");
    assert_eq!(args[pos + 1], artifacts.display().to_string());

    let settings = args
        .iter()
        .position(|a| a == "--settings")
        .map(|pos| args[pos + 1].clone())
        .expect("the engine is asked by the CLI's own pre-write hook");
    assert!(
        settings.contains("PreToolUse") && settings.contains("rail claude-code before-write"),
        "the hook runs `yunta rail` before every writing tool: {settings}"
    );
    assert!(
        !dir.path().join(".claude").exists(),
        "nothing of the session's configuration lands under the worktree"
    );
}

/// Before the session ends its turn, the CLI asks the engine, and waits
/// for it as long as a call that runs the task's checks may take; the
/// CLI's own cap on how often a stop is sent back is lifted, because the
/// engine sends one back at most once for one state of the work.
#[tokio::test]
async fn a_claude_session_asks_the_engine_before_it_stops() {
    let dir = tempfile::tempdir().unwrap();
    let env_file = dir.path().join("env.txt");
    let mut req = request(dir.path().to_path_buf());
    req.budget.timeout = Some(std::time::Duration::from_secs(1500));
    req.env.plain.insert(
        "CLAUDE_STUB_ENV_FILE".to_string(),
        env_file.display().to_string(),
    );
    let args = argv_for(dir.path(), req).await;

    let settings = args
        .iter()
        .position(|a| a == "--settings")
        .map(|pos| args[pos + 1].clone())
        .unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
    let stop = &settings["hooks"]["Stop"][0]["hooks"][0];
    assert!(stop["command"]
        .as_str()
        .is_some_and(|command| command.ends_with("rail claude-code before-stop")));
    assert_eq!(stop["timeout"], 1500);
    let env = tokio::fs::read_to_string(&env_file).await.unwrap();
    assert!(
        env.lines()
            .any(|line| line == "CLAUDE_CODE_STOP_HOOK_BLOCK_CAP=0"),
        "{env}"
    );
}

/// An isolated session loads none of the person's own settings and no MCP
/// server but the run's; an inheriting one loads them as the CLI always
/// does.
#[tokio::test]
async fn an_isolated_session_loads_no_setting_source_and_no_mcp_server_but_the_run_s() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path().to_path_buf());
    req.cli_config = yunta_core::CliConfig::Isolated;
    let isolated = argv_for(dir.path(), req).await;
    let sources = isolated
        .iter()
        .position(|arg| arg == "--setting-sources")
        .expect("an isolated session names its setting sources");
    assert_eq!(isolated[sources + 1], "", "and names none: {isolated:?}");
    assert!(
        isolated.contains(&"--strict-mcp-config".to_string()),
        "{isolated:?}"
    );

    let dir = tempfile::tempdir().unwrap();
    let inherited = argv_for(dir.path(), request(dir.path().to_path_buf())).await;
    assert!(
        !inherited
            .iter()
            .any(|arg| arg == "--setting-sources" || arg == "--strict-mcp-config"),
        "{inherited:?}"
    );
}

#[tokio::test]
async fn a_fence_with_no_roots_widens_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let args = argv_for(dir.path(), request(dir.path().to_path_buf())).await;
    assert!(
        !args.iter().any(|a| a == "--add-dir"),
        "a session with nothing to write outside its worktree needs no widening: {args:?}"
    );
}

/// `read_only` means the node does not touch the project. Its own
/// declared file is the node's output, written to a root the fence keeps
/// writable — so the writing tools stay exactly when there is such a
/// root, and go when there is not.
#[tokio::test]
async fn a_read_only_claude_session_keeps_write_only_for_its_declared_files() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("run/artifacts");

    let mut req = request(dir.path().to_path_buf());
    req.permissions = PermissionProfile::ReadOnly;
    req.fence = Fence::read_only(vec![artifacts], Advice::ReportFinding);
    let args = argv_for(dir.path(), req).await;
    let pos = args.iter().position(|a| a == "--tools").unwrap();
    assert!(
        args[pos + 1].split(',').any(|tool| tool == "Write"),
        "read-only must still produce what it declares: {}",
        args[pos + 1]
    );

    let mut req = request(dir.path().to_path_buf());
    req.permissions = PermissionProfile::ReadOnly;
    req.fence = Fence::read_only(Vec::new(), Advice::ReportFinding);
    let args = argv_for(dir.path(), req).await;
    let pos = args.iter().position(|a| a == "--tools").unwrap();
    assert!(
        !args[pos + 1].split(',').any(|tool| tool == "Write"),
        "with nothing declared there is nothing at all to write: {}",
        args[pos + 1]
    );
}

/// Asserts that the file at `path` is readable by its owner alone.
#[cfg(unix)]
fn assert_owner_only(path: &str) {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "{path} is readable by others");
}

#[tokio::test]
async fn the_per_run_tools_reach_the_session_without_the_token_on_the_command_line() {
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().join("run/scratch");
    std::fs::create_dir_all(&scratch).unwrap();

    let mut req = request(dir.path().to_path_buf());
    req.scratch_dir = scratch.clone();
    req.run_tools_endpoint = Some(yunta_core::port::RunToolsEndpoint {
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
    // The config holds the bearer, so no other user may read it.
    #[cfg(unix)]
    assert_owner_only(&args[pos + 1]);
    assert!(
        config["mcpServers"]["yunta"].is_null(),
        "the per-run server never takes the name a person's own entry takes: {config:#}"
    );
    let server = &config["mcpServers"]["yunta-run"];
    assert_eq!(server["url"], "http://127.0.0.1:54321/mcp");
    assert_eq!(
        server["headers"]["Authorization"], "Bearer s3cr3t-token-value",
        "the listener authenticates every call"
    );
    assert_eq!(
        server["timeout"], 3_600_000,
        "a call that runs criteria is waited for as long as a session with no wall clock may run"
    );

    // The token is secret material: argv is world-readable through `ps`.
    assert!(
        !args.iter().any(|a| a.contains("s3cr3t-token-value")),
        "the token must never reach the process list: {args:?}"
    );
    // Mounting a server the profile then forbids would be a tool the
    // agent is told to call and cannot.
    assert!(
        args.iter().any(|a| a == "mcp__yunta-run__*"),
        "every tool of the mounted server is allowed, so this adapter \
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

#[tokio::test]
async fn interleaved_tool_results_are_matched_by_id_and_ignore_other_servers() {
    let events = events_of(&[
        INIT_LINE,
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"a","name":"mcp__yunta-run__yunta_submit_questions","input":{"secret":"top-secret"}},{"type":"tool_use","id":"b","name":"mcp__yunta-run__yunta_check_artifact","input":{}},{"type":"tool_use","id":"c","name":"mcp__other__yunta_submit_questions","input":{}}]}}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"b","is_error":true,"content":"top-secret"},{"type":"tool_result","tool_use_id":"c","is_error":true,"content":"top-secret"}]}}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"a","is_error":true,"content":"top-secret"}]}}"#,
        RESULT_LINE,
    ]).await;
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
                yunta_core::RunTool::CheckArtifact,
                yunta_core::events::RunToolFailureCause::CallFailed
            ),
            (
                yunta_core::RunTool::Submit(yunta_core::ArtifactKind::Questions),
                yunta_core::events::RunToolFailureCause::CallFailed
            ),
        ]
    );
    assert!(!format!("{failed:?}").contains("top-secret"));
}

/// Usage is reported as the session spends it: each assistant message
/// once — the CLI repeats its usage on every line carrying one of its
/// blocks — and the result's totals only for what the messages did not
/// say. The sum is the result's, with the cache read inside the input.
#[tokio::test]
async fn usage_is_reported_per_message_once_and_reconciled_by_the_result() {
    let dir = tempfile::tempdir().unwrap();
    let usage = |input: u64, read: u64, created: u64, output: u64| {
        format!(
            r#"{{"input_tokens":{input},"cache_read_input_tokens":{read},"cache_creation_input_tokens":{created},"output_tokens":{output}}}"#
        )
    };
    let assistant = |id: &str, usage: &str| {
        format!(r#"{{"type":"assistant","message":{{"id":"{id}","content":[],"usage":{usage}}}}}"#)
    };
    let (first, second) = (usage(3, 0, 8599, 8), usage(1, 8599, 473, 2));
    let result = format!(
        r#"{{"type":"result","is_error":false,"result":"done","usage":{}}}"#,
        usage(4, 8599, 9072, 896)
    );
    let lines = [
        INIT_LINE.to_string(),
        assistant("msg-1", &first),
        assistant("msg-1", &first),
        assistant("msg-2", &second),
        result,
    ];
    let events = spawn_with(
        &dir,
        request(dir.path().to_path_buf()),
        &lines.each_ref().map(String::as_str),
    )
    .await;

    let spent: Vec<(u64, u64, u64)> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::Usage {
                input_tokens,
                output_tokens,
                cached_input_tokens,
            } => Some((
                input_tokens.unwrap_or(0),
                cached_input_tokens.unwrap_or(0),
                output_tokens.unwrap_or(0),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(spent.len(), 3, "two messages and the rest: {spent:?}");
    let total = spent
        .iter()
        .fold((0, 0, 0), |(i, c, o), (a, b, d)| (i + a, c + b, o + d));
    assert_eq!(total, (4 + 8599 + 9072, 8599, 896));
    assert!(spent.iter().all(|(input, cached, _)| cached <= input));
}

/// A field the CLI filled with something that cannot be a session id
/// fails the session, and the failure keeps what rejected the value so a
/// reader following the chain reaches the rule it broke.
#[tokio::test]
async fn a_session_id_that_cannot_be_one_fails_keeping_what_rejected_it() {
    let events = events_of(&[
        r#"{"type":"system","subtype":"init","session_id":"","model":"claude-sonnet-5"}"#,
    ])
    .await;

    let AgentEvent::Failed { error, cause } = &events[0] else {
        panic!("expected Failed, got {:?}", events[0]);
    };
    assert!(!cause.retryable(), "not a failure to retry: {error}");
    let described = yunta_core::describe(error);
    assert!(
        described.starts_with("the CLI's init line's `session_id`"),
        "the failure names the field: {described}"
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

/// The settings a session is handed carry the sandbox its commands run in:
/// one that fails rather than run unconfined, whose network the request
/// opens or keeps closed, that writes nothing of the checkout under
/// `read_only`, and that a `full` session runs without.
#[tokio::test]
async fn a_session_s_commands_run_in_the_sandbox_its_profile_and_request_say() {
    let sandbox = |profile: PermissionProfile, network: bool| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut req = request(dir.path().to_path_buf());
        req.permissions = profile;
        req.network = network;
        let args = argv_for(dir.path(), req).await;
        let settings = args
            .iter()
            .position(|arg| arg == "--settings")
            .map(|pos| args[pos + 1].clone())
            .expect("a session is handed its settings");
        let settings: serde_json::Value = serde_json::from_str(&settings).unwrap();
        (settings["sandbox"].clone(), dir)
    };

    let (open, _dir) = sandbox(PermissionProfile::Edit, true).await;
    assert_eq!(open["enabled"], true);
    assert_eq!(open["failIfUnavailable"], true);
    assert_eq!(open["allowUnsandboxedCommands"], false);
    assert_eq!(open["network"]["allowedDomains"], serde_json::json!(["*"]));
    assert_eq!(open["network"]["allowLocalBinding"], true);

    let (closed, _dir) = sandbox(PermissionProfile::Edit, false).await;
    assert_eq!(closed["network"], serde_json::json!({}));

    let (read_only, dir) = sandbox(PermissionProfile::ReadOnly, true).await;
    assert_eq!(read_only["network"], serde_json::json!({}));
    assert_eq!(
        read_only["filesystem"]["denyWrite"],
        serde_json::json!([dir.path()])
    );

    let (full, _dir) = sandbox(PermissionProfile::Full, true).await;
    assert!(full.is_null(), "{full}");
}

/// The hook's refusal comes back through the conversation as an errored
/// tool result, but the engine already recorded it when the hook asked:
/// the stream reporting it again would count one refusal twice.
#[tokio::test]
async fn a_refusal_in_the_stream_is_left_to_the_engine_that_judged_it() {
    let dir = tempfile::tempdir().unwrap();
    let refusal = yunta_core::fence::Refusal {
        target: dir.path().join("docs/readme.md"),
        worktree: dir.path().to_path_buf(),
        allowed: Some(vec!["src/**".into()]),
        roots: Vec::new(),
        advice: yunta_core::fence::Advice::ReportFinding,
        protection: None,
        naming: yunta_core::ToolNaming::Bare,
    };
    let user_line = serde_json::json!({
        "type": "user",
        "message": { "content": [{
            "type": "tool_result",
            "is_error": true,
            "content": refusal.to_string(),
        }] },
    })
    .to_string();

    let events = spawn_with(
        &dir,
        request(dir.path().to_path_buf()),
        &[INIT_LINE, &user_line, RESULT_LINE],
    )
    .await;
    let without_the_refusal = spawn_with(
        &dir,
        request(dir.path().to_path_buf()),
        &[INIT_LINE, RESULT_LINE],
    )
    .await;

    assert_eq!(
        events, without_the_refusal,
        "the refusal adds no event to the stream: {events:?}"
    );
}
