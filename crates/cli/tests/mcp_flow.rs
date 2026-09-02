//! `yunta mcp` end to end: a real MCP client, talking real
//! stdio JSON-RPC, against the real binary spawned as a child process —
//! the same combination rmcp documents for testing a stdio server from
//! the outside. Proves the whole chain works, not just that the
//! dispatch functions return the right strings in isolation.

use std::path::Path;

use rmcp::model::CallToolRequestParams;
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use serde_json::json;
use yunta_adapters::signal::{signal_process, Signal};
use yunta_core::Pid;

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join(".gitkeep"), "").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn tool_text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .map(|text| text.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn yunta_mcp_lists_runs_and_drives_a_workflow_to_completion() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join(".yunta/workflows/greet.yaml"),
        r#"
name: greet
nodes:
  - id: hello
    kind: bash
    run: "echo hello > greeting.txt"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "catalog"]);

    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    command
        .arg("mcp")
        .current_dir(&repo)
        .env("YUNTA_HOME", &home);
    let transport = TokioChildProcess::new(command).unwrap();
    let client = ().serve(transport).await.unwrap();

    let tools = client.list_tools(None).await.unwrap();
    let names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_ref()).collect();
    for expected in [
        "list_workflows",
        "run_workflow",
        "workflow_status",
        "resume_run",
        "resolve_gate",
    ] {
        assert!(
            names.contains(&expected),
            "missing tool `{expected}`: {names:?}"
        );
    }

    let catalog = client
        .call_tool(CallToolRequestParams::new("list_workflows"))
        .await
        .unwrap();
    assert!(
        !catalog.is_error.unwrap_or(false),
        "got: {}",
        tool_text(&catalog)
    );
    assert!(
        tool_text(&catalog).contains("greet"),
        "got: {}",
        tool_text(&catalog)
    );

    let run = client
        .call_tool(
            CallToolRequestParams::new("run_workflow")
                .with_arguments(json!({"workflow": "greet"}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let run_text = tool_text(&run);
    assert!(!run.is_error.unwrap_or(false), "got: {run_text}");
    let run_id = run_text
        .strip_prefix("run_id: ")
        .expect("run_workflow must return a run_id")
        .trim()
        .to_string();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let status = client
            .call_tool(
                CallToolRequestParams::new("workflow_status")
                    .with_arguments(json!({"run_id": run_id}).as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        if tool_text(&status).contains("finished") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the run never reached finished via workflow_status: {}",
            tool_text(&status)
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(
        std::fs::read_to_string(repo.join("greeting.txt"))
            .unwrap()
            .trim(),
        "hello"
    );

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn yunta_mcp_resolve_gate_answers_an_exhausted_reroute() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: hopeless
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "touch fixed.txt"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixtures"]);

    // Paused with `yunta run` directly (no MCP involved yet) — proves
    // resolve_gate answers a run that some *other* process created.
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["run", "wf.yaml"])
        .current_dir(&repo)
        .env("YUNTA_HOME", &home)
        .output()
        .unwrap();
    let run_id = String::from_utf8_lossy(&run.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix("run ")
                .and_then(|rest| rest.split(':').next())
                .map(str::to_string)
        })
        .expect("run id in output");

    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    command
        .arg("mcp")
        .current_dir(&repo)
        .env("YUNTA_HOME", &home);
    let transport = TokioChildProcess::new(command).unwrap();
    let client = ().serve(transport).await.unwrap();

    let resolved = client
        .call_tool(
            CallToolRequestParams::new("resolve_gate").with_arguments(
                json!({"run_id": run_id, "option": "retry"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert!(
        !resolved.is_error.unwrap_or(false),
        "got: {}",
        tool_text(&resolved)
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let status = client
            .call_tool(
                CallToolRequestParams::new("workflow_status")
                    .with_arguments(json!({"run_id": run_id}).as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        if tool_text(&status).contains("finished") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the run never reached finished after resolve_gate: {}",
            tool_text(&status)
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn a_run_survives_yunta_mcp_being_killed_and_a_fresh_session_confirms_it() {
    // No run's own life depends on the MCP session that
    // created it. Killing `yunta mcp` outright — not a graceful
    // shutdown — must not touch the run it started via `run_workflow`'s
    // own detached child.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    );
    write(
        &repo.join(".yunta/workflows/slow.yaml"),
        r#"
name: slow
nodes:
  - id: work
    kind: bash
    run: "sleep 2 && echo done > done.txt"
"#,
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "catalog"]);

    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    command
        .arg("mcp")
        .current_dir(&repo)
        .env("YUNTA_HOME", &home);
    let transport = TokioChildProcess::new(command).unwrap();
    let mcp_pid = transport.id().expect("child process must have a pid");
    let client = ().serve(transport).await.unwrap();

    let run = client
        .call_tool(
            CallToolRequestParams::new("run_workflow")
                .with_arguments(json!({"workflow": "slow"}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let run_id = tool_text(&run)
        .strip_prefix("run_id: ")
        .expect("run_workflow must return a run_id")
        .trim()
        .to_string();

    // Kill `yunta mcp` outright — no graceful shutdown.
    signal_process(
        Pid::try_from(mcp_pid).expect("a spawned child has a positive pid"),
        Signal::SIGKILL,
    )
    .expect("`yunta mcp` must be alive to be killed");
    drop(client);

    // A brand new MCP session, sharing nothing with the killed one,
    // confirms the run kept going and eventually finished.
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    command
        .arg("mcp")
        .current_dir(&repo)
        .env("YUNTA_HOME", &home);
    let transport = TokioChildProcess::new(command).unwrap();
    let fresh_client = ().serve(transport).await.unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let status = fresh_client
            .call_tool(
                CallToolRequestParams::new("workflow_status")
                    .with_arguments(json!({"run_id": run_id}).as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        if tool_text(&status).contains("finished") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the run must survive the MCP session that created it being killed: {}",
            tool_text(&status)
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(
        std::fs::read_to_string(repo.join("done.txt"))
            .unwrap()
            .trim(),
        "done"
    );
    fresh_client.cancel().await.unwrap();
}
