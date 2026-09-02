//! `mcp:` context source — exercised against a real MCP
//! server speaking streamable-HTTP, run in-process on a loopback port
//! for the test's own duration. No stdio, no stub: this drives the real
//! `context_resolve.rs` client code path against a real (if minimal)
//! `rmcp` server, the same library the client itself is built on.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ServerCapabilities,
    ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use tokio_util::sync::CancellationToken;
use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::{AdapterId, Clock, ConfigLayer, McpServerConfig, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, RunEnv, RunTerminal,
    DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .expect("valid timestamp")
            .with_timezone(&Utc)
    }
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo(dir: &std::path::Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join(".gitkeep"), "").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

/// Echoes the `query` argument back inside its own response text, so a
/// test can prove round-trip content without guessing at a fixed reply.
/// Rejects any tool name but `query` — this recorte's own documented
/// mapping (`context_resolve.rs`'s module doc) is `tools/call` on a tool
/// literally named that.
#[derive(Clone)]
struct ToyMcpServer;

impl ServerHandler for ToyMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if request.name != "query" {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "unknown tool `{}`",
                request.name
            ))])
            .into());
        }
        let query = request
            .arguments
            .as_ref()
            .and_then(|args| args.get("query"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "MARKER-MCP-RESPONSE for: {query}"
        ))])
        .into())
    }
}

/// Starts the toy server on an OS-assigned loopback port and returns its
/// full `mcp_servers:`-style URL plus the cancellation token that tears
/// it down.
async fn start_toy_server() -> (String, CancellationToken) {
    let ct = CancellationToken::new();
    let service = StreamableHttpService::new(
        || Ok(ToyMcpServer),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default().with_cancellation_token(ct.child_token()),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://127.0.0.1:{}/mcp",
        listener.local_addr().unwrap().port()
    );
    let shutdown_ct = ct.clone();
    tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move { shutdown_ct.cancelled().await })
            .await
            .unwrap();
    });
    // Give the listener a moment to actually accept before the client dials.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (url, ct)
}

const CONTEXT_WORKFLOW: &str = r#"
name: ctx-mcp
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: "Do the thing."
    context:
      - mcp: { server: toy, query: "MARKER-MCP-QUERY" }
"#;

async fn run_with_config(
    workflow_yaml: &str,
    fixture_yaml: &str,
    config: ConfigLayer,
) -> (
    RunTerminal,
    Vec<yunta_core::events::StoredEvent>,
    std::path::PathBuf,
    tempfile::TempDir,
) {
    let root = tempfile::tempdir().unwrap();
    let worktree = root.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    init_repo(&worktree);
    let runs_root = root.path().join("runs");
    let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
    let run_id = RunId::from("run-test-1");

    let workflow: Workflow = serde_yaml::from_str(workflow_yaml).unwrap();
    let manifest =
        build_manifest(&workflow, &config, &worktree, &worktree, &HashMap::new()).unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &runs_root,
            mode: &"default".into(),
            promoted_from: None,
        },
        &storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    let adapter = MockAdapter::from_yaml(fixture_yaml).unwrap();
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), Arc::new(adapter));

    let report = execute_run(RunEnv {
        run_id: &run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &worktree,
        adapters: &adapters,
        storage: &storage.async_handle(),
        clock: &FixedClock,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
    })
    .await
    .unwrap();

    let events = storage.events_for_run(&run_id).unwrap();
    (report.terminal, events, run_dir, root)
}

fn config_with_server(url: &str, auth_env: Option<&str>) -> ConfigLayer {
    ConfigLayer {
        runners: Some(HashMap::from([(
            "executor".into(),
            vec![yunta_core::RunnerCandidate {
                adapter: "mock".into(),
                model: "mock-model".into(),
                agent: None,
            }],
        )])),
        mcp_servers: Some(HashMap::from([(
            "toy".to_string(),
            McpServerConfig {
                url: url.to_string(),
                auth_env: auth_env.map(str::to_string),
            },
        )])),
        ..Default::default()
    }
}

#[tokio::test]
async fn an_mcp_source_resolves_the_toy_server_s_response_and_is_replayable() {
    let (url, ct) = start_toy_server().await;
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-MCP-RESPONSE for: MARKER-MCP-QUERY\"\n    outcome: { type: completed, summary: ok }\n";

    let (terminal, events, run_dir, _root) =
        run_with_config(CONTEXT_WORKFLOW, fixture, config_with_server(&url, None)).await;
    ct.cancel();

    assert_eq!(terminal, RunTerminal::Finished);

    let sources: Vec<_> = events
        .iter()
        .find_map(|e| match (&e.node_id, e.payload()) {
            (Some(n), Some(yunta_core::events::EventPayload::ContextAssembled(p)))
                if n.as_str() == "ask" =>
            {
                Some(p.sources.clone())
            }
            _ => None,
        })
        .expect("context_assembled event for `ask`");
    assert_eq!(sources[0].kind, "mcp");

    let materialized = run_dir
        .join("context")
        .join(&sources[0].content_hash)
        .join("content");
    let bytes = std::fs::read(&materialized).expect("materialized mcp response");
    assert_eq!(yunta_core::sha256_hex(&bytes), sources[0].content_hash);
    assert!(String::from_utf8_lossy(&bytes).contains("MARKER-MCP-RESPONSE"));
}

#[tokio::test]
async fn an_unknown_mcp_server_fails_the_node_before_any_connection_attempt() {
    let config = ConfigLayer {
        runners: Some(HashMap::from([(
            "executor".into(),
            vec![yunta_core::RunnerCandidate {
                adapter: "mock".into(),
                model: "mock-model".into(),
                agent: None,
            }],
        )])),
        // No `mcp_servers:` declared at all.
        ..Default::default()
    };
    // No sessions declared — if the node somehow tried to dispatch a
    // session before failing, the mock would error loudly instead.
    let (terminal, _events, _run_dir, _root) =
        run_with_config(CONTEXT_WORKFLOW, "sessions: []", config).await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("toy"), "got: {reason}");
        }
        other => panic!("expected the run to pause citing the unknown server, got {other:?}"),
    }
}

#[tokio::test]
async fn a_missing_auth_env_var_fails_the_node_before_any_connection_attempt() {
    let config = config_with_server(
        "http://127.0.0.1:1/mcp", // never dialed — the env check comes first
        Some("YUNTA_TEST_MCP_TOKEN_DOES_NOT_EXIST"),
    );
    std::env::remove_var("YUNTA_TEST_MCP_TOKEN_DOES_NOT_EXIST");
    let (terminal, _events, _run_dir, _root) =
        run_with_config(CONTEXT_WORKFLOW, "sessions: []", config).await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(
                reason.contains("YUNTA_TEST_MCP_TOKEN_DOES_NOT_EXIST"),
                "got: {reason}"
            );
        }
        other => panic!("expected the run to pause citing the missing env var, got {other:?}"),
    }
}
