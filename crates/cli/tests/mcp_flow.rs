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
use yunta_testkit::{git, init_repo, wait_until_async, write, yunta_in};

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

    let last_status = std::cell::RefCell::new(String::new());
    wait_until_async(
        || {
            let client = &client;
            let last_status = &last_status;
            let run_id = &run_id;
            async move {
                let status = client
                    .call_tool(
                        CallToolRequestParams::new("workflow_status")
                            .with_arguments(json!({"run_id": run_id}).as_object().unwrap().clone()),
                    )
                    .await
                    .unwrap();
                *last_status.borrow_mut() = tool_text(&status);
                last_status.borrow().contains("finished")
            }
        },
        || {
            format!(
                "the run never reached finished via workflow_status: {}",
                last_status.borrow()
            )
        },
    )
    .await;
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

    let last_status = std::cell::RefCell::new(String::new());
    wait_until_async(
        || {
            let client = &client;
            let last_status = &last_status;
            let run_id = &run_id;
            async move {
                let status = client
                    .call_tool(
                        CallToolRequestParams::new("workflow_status")
                            .with_arguments(json!({"run_id": run_id}).as_object().unwrap().clone()),
                    )
                    .await
                    .unwrap();
                *last_status.borrow_mut() = tool_text(&status);
                last_status.borrow().contains("finished")
            }
        },
        || {
            format!(
                "the run never reached finished after resolve_gate: {}",
                last_status.borrow()
            )
        },
    )
    .await;

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
    run: "echo done > done.txt"
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

    let last_status = std::cell::RefCell::new(String::new());
    wait_until_async(
        || {
            let client = &fresh_client;
            let last_status = &last_status;
            let run_id = &run_id;
            async move {
                let status = client
                    .call_tool(
                        CallToolRequestParams::new("workflow_status")
                            .with_arguments(json!({"run_id": run_id}).as_object().unwrap().clone()),
                    )
                    .await
                    .unwrap();
                *last_status.borrow_mut() = tool_text(&status);
                last_status.borrow().contains("finished")
            }
        },
        || {
            format!(
                "the run must survive the MCP session that created it being killed: {}",
                last_status.borrow()
            )
        },
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(repo.join("done.txt"))
            .unwrap()
            .trim(),
        "done"
    );
    fresh_client.cancel().await.unwrap();
}

/// An upstream repo holding one pure-`bash` pack (`acme/review-pack`),
/// committed so `pack add` can clone and vendor it.
fn write_pack(dir: &Path) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        "name: review-pack\n\
         publisher: acme\n\
         version: 1.0.0\n\
         description: a runnable pack\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/review.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("workflows/review.yaml"),
        "name: review\ndescription: pack-provided review\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "v1"]);
}

#[tokio::test]
async fn workflow_status_returns_versioned_json() {
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
        "name: greet\nnodes:\n  - id: hello\n    kind: bash\n    run: \"true\"\n",
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "catalog"]);

    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    command
        .arg("mcp")
        .current_dir(&repo)
        .env("YUNTA_HOME", &home);
    let client = ().serve(TokioChildProcess::new(command).unwrap()).await.unwrap();

    let run = client
        .call_tool(
            CallToolRequestParams::new("run_workflow")
                .with_arguments(json!({"workflow": "greet"}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let run_id = tool_text(&run)
        .strip_prefix("run_id: ")
        .expect("run_workflow returns a run_id")
        .trim()
        .to_string();

    // workflow_status is machine-readable: a versioned JSON document —
    // the same DTO `yunta status --json` prints — not the human status
    // text a client would have to scrape.
    let status = client
        .call_tool(
            CallToolRequestParams::new("workflow_status")
                .with_arguments(json!({"run_id": run_id}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let text = tool_text(&status);
    let value: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("workflow_status must be JSON: {e}\ngot: {text}"));
    assert!(
        value.get("schema_version").is_some(),
        "the DTO carries a schema_version: {text}"
    );
    assert_eq!(
        value.get("run_id").and_then(|v| v.as_str()),
        Some(run_id.as_str()),
        "the DTO names its run: {text}"
    );

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn run_workflow_accepts_pack_names() {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    write_pack(&upstream);

    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let add = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(
        add.status.success(),
        "pack add: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    command
        .arg("mcp")
        .current_dir(&repo)
        .env("YUNTA_HOME", &home);
    let client = ().serve(TokioChildProcess::new(command).unwrap()).await.unwrap();

    // A `publisher/name` names a vendored pack's workflow, resolved the
    // same way `yunta run acme/review` resolves it — the control plane
    // reaches the whole catalog, not only the repo's own
    // `.yunta/workflows/`.
    let run = client
        .call_tool(
            CallToolRequestParams::new("run_workflow").with_arguments(
                json!({"workflow": "acme/review"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let text = tool_text(&run);
    assert!(
        !run.is_error.unwrap_or(false),
        "run_workflow rejected a pack name: {text}"
    );
    assert!(
        text.strip_prefix("run_id: ")
            .map(|id| !id.trim().is_empty())
            .unwrap_or(false),
        "run_workflow returns a run_id for a pack workflow: {text}"
    );

    client.cancel().await.unwrap();
}

/// The pids of any defunct (state `Z`) child of `ppid`, read from
/// `/proc` — a finished process its parent never reaped. The `comm`
/// field can hold spaces and parens, so the fields after the last `)`
/// are state, then ppid.
fn zombie_children(ppid: u32) -> Vec<u32> {
    let mut zombies = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return zombies;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        let Some((_, after_comm)) = stat.rsplit_once(')') else {
            continue;
        };
        let fields: Vec<&str> = after_comm.split_whitespace().collect();
        let state = fields.first().copied().unwrap_or_default();
        let parent = fields.get(1).and_then(|f| f.parse::<u32>().ok());
        if state == "Z" && parent == Some(ppid) {
            zombies.push(pid);
        }
    }
    zombies
}

#[tokio::test]
async fn finished_detached_runs_leave_no_zombie() {
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
        &repo.join(".yunta/workflows/quick.yaml"),
        "name: quick\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n",
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "catalog"]);

    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    command
        .arg("mcp")
        .current_dir(&repo)
        .env("YUNTA_HOME", &home);
    let transport = TokioChildProcess::new(command).unwrap();
    let mcp_pid = transport.id().expect("the mcp server has a pid");
    let client = ().serve(transport).await.unwrap();

    let run = client
        .call_tool(
            CallToolRequestParams::new("run_workflow")
                .with_arguments(json!({"workflow": "quick"}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let run_id = tool_text(&run)
        .strip_prefix("run_id: ")
        .expect("run_workflow returns a run_id")
        .trim()
        .to_string();

    // Drive the detached run to completion.
    let last_status = std::cell::RefCell::new(String::new());
    wait_until_async(
        || {
            let client = &client;
            let last_status = &last_status;
            let run_id = &run_id;
            async move {
                let status = client
                    .call_tool(
                        CallToolRequestParams::new("workflow_status")
                            .with_arguments(json!({"run_id": run_id}).as_object().unwrap().clone()),
                    )
                    .await
                    .unwrap();
                *last_status.borrow_mut() = tool_text(&status);
                last_status.borrow().contains("finished")
            }
        },
        || format!("the detached run never finished: {}", last_status.borrow()),
    )
    .await;

    // The finished detached child is a child of the long-lived server: it
    // must be reaped, never left defunct. Give the reaper a moment.
    wait_until_async(
        || async { zombie_children(mcp_pid).is_empty() },
        || {
            format!(
                "a finished detached run is a zombie under the server: {:?}",
                zombie_children(mcp_pid)
            )
        },
    )
    .await;

    client.cancel().await.unwrap();
}
