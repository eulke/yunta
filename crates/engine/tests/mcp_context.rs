//! `mcp:` context source — exercised against a real MCP
//! server speaking streamable-HTTP, run in-process on a loopback port
//! for the test's own duration. No stdio, no stub: this drives the real
//! `context_resolve.rs` client code path against a real (if minimal)
//! `rmcp` server, the same library the client itself is built on.

use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ServerCapabilities,
    ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use tokio_util::sync::CancellationToken;
use yunta_core::events::NodeEvent;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

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
    // The listener is already bound, so a client that dials before `serve`
    // accepts simply waits in the socket backlog — no pause needed here.
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

/// A config layer declaring the toy server under `mcp_servers:`,
/// alongside the `executor` runner the workflow's node binds to.
fn config_with_server(url: &str, auth_env: Option<&str>) -> String {
    let auth = match auth_env {
        Some(var) => format!("    auth_env: {var}\n"),
        None => String::new(),
    };
    format!(
        "\
runners:
  executor:
    - {{ adapter: mock, model: mock-model }}
mcp_servers:
  toy:
    url: \"{url}\"
{auth}"
    )
}

#[tokio::test]
async fn an_mcp_source_resolves_the_toy_server_s_response_and_is_replayable() {
    let (url, ct) = start_toy_server().await;
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-MCP-RESPONSE for: MARKER-MCP-QUERY\"\n    outcome: { type: completed, summary: ok }\n";

    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(CONTEXT_WORKFLOW, fixture, &config_with_server(&url, None))
        .await;
    ct.cancel();

    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.events();
    let sources: Vec<_> = events
        .iter()
        .find_map(|e| match (&e.node_id, e.payload()) {
            (
                Some(n),
                Some(yunta_core::events::EventPayload::Node(NodeEvent::ContextAssembled(p))),
            ) if n.as_str() == "ask" => Some(p.sources.clone()),
            _ => None,
        })
        .expect("context_assembled event for `ask`");
    assert_eq!(sources[0].kind, "mcp");

    let bytes = bench
        .object(&sources[0].content_hash)
        .expect("materialized mcp response");
    assert_eq!(yunta_core::sha256_hex(&bytes), sources[0].content_hash);
    assert!(String::from_utf8_lossy(&bytes).contains("MARKER-MCP-RESPONSE"));
}

#[tokio::test]
async fn an_unknown_mcp_server_fails_the_node_before_any_connection_attempt() {
    // No sessions declared — if the node somehow tried to dispatch a
    // session before failing, the mock would error loudly instead. The
    // config behind `run` declares no `mcp_servers:` at all.
    let RunReport { terminal, .. } = Bench::new().run(CONTEXT_WORKFLOW, "sessions: []").await;
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
    // The run carries no secrets, so the variable the server names is
    // one nothing can answer with.
    let RunReport { terminal, .. } = Bench::new()
        .run_with_secrets(CONTEXT_WORKFLOW, "sessions: []", &config, &[])
        .await;
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
