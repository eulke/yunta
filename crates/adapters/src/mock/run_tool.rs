//! The mock's client leg: one real MCP call against the session's own
//! per-run endpoint.
//!
//! A fixture that scripts a run tool exercises the engine's listener over
//! the wire, exactly where a real CLI would place the call — the mock is
//! an adapter, not a stub of one, so what a fixture proves about the run
//! tools is what an agent would meet. What comes back is reduced to a
//! digest: the audit stream carries evidence that a call happened and
//! what it answered, never the content itself.

use std::time::Duration;

use yunta_core::port::RunToolsEndpoint;

/// Why the mock's own MCP call failed — the session fails with it.
#[derive(Debug, thiserror::Error)]
pub(super) enum RunToolCallError {
    #[error(
        "fixture step run_tool `{tool}` but this session has no run_tools_endpoint — the \
         engine never offered one (missing `run_tools` capability, or the listener wasn't \
         opened)"
    )]
    NoEndpoint { tool: String },
    #[error("run_tool `{tool}`: cannot reach the per-run endpoint")]
    Connect {
        tool: String,
        #[source]
        source: Box<rmcp::service::ClientInitializeError>,
    },
    #[error("run_tool `{tool}` failed")]
    Call {
        tool: String,
        #[source]
        source: Box<rmcp::service::ServiceError>,
    },
    #[error("run_tool `{tool}` returned an error: {text}")]
    Refused { tool: String, text: String },
    /// The call did not answer within the call timeout the CLI would give it.
    #[error("run_tool `{tool}` did not answer within {}ms", .within.as_millis())]
    TimedOut { tool: String, within: Duration },
}

impl RunToolCallError {
    /// A `tools/call` request was sent to the per-run server and answered.
    /// A call that timed out was never answered, so it does not count.
    pub(super) fn reached_call(&self) -> bool {
        matches!(self, Self::Call { .. } | Self::Refused { .. })
    }
}

/// What a CLI does with a per-run server before its first turn: connect and
/// list the tools, within the CLI's own call timeout. A listing that fails
/// is a server the session does not hold, which the engine reads from its
/// own listener.
pub(super) async fn list_run_tools(endpoint: &RunToolsEndpoint, within: Duration) {
    if tokio::time::timeout(within, listing(endpoint))
        .await
        .is_err()
    {
        tracing::debug!("a mock session's run-tool listing ran past its timeout");
    }
}

/// The listing itself: connect, list the tools, and close.
async fn listing(endpoint: &RunToolsEndpoint) {
    use rmcp::ServiceExt;

    let config =
        rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(
            endpoint.url.clone(),
        )
        .auth_header(endpoint.token.expose().clone());
    let transport = rmcp::transport::StreamableHttpClientTransport::with_client(
        reqwest::Client::default(),
        config,
    );
    let Ok(client) = ().serve(transport).await else {
        return;
    };
    if client.list_all_tools().await.is_err() {
        tracing::debug!("a mock session could not list its run tools");
    }
    if client.cancel().await.is_err() {
        tracing::debug!("a mock session's tool client did not close");
    }
}

/// The mock's own MCP client leg: one `tools/call` against the session's
/// per-run endpoint, exactly as a real CLI would place it, within the CLI's
/// own call timeout. Returns a short digest of the response for the audit
/// stream (`ToolUse.target` — never full content), or the error that fails
/// the session.
pub(super) async fn call_run_tool(
    endpoint: Option<&RunToolsEndpoint>,
    tool: &str,
    arguments: serde_json::Map<String, serde_json::Value>,
    within: Duration,
) -> std::result::Result<String, RunToolCallError> {
    let Some(endpoint) = endpoint else {
        return Err(RunToolCallError::NoEndpoint {
            tool: tool.to_string(),
        });
    };
    match tokio::time::timeout(within, exchange(endpoint, tool, arguments)).await {
        Ok(result) => result,
        Err(_) => Err(RunToolCallError::TimedOut {
            tool: tool.to_string(),
            within,
        }),
    }
}

/// One call from start to finish: connect, `tools/call`, and close.
async fn exchange(
    endpoint: &RunToolsEndpoint,
    tool: &str,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> std::result::Result<String, RunToolCallError> {
    use rmcp::ServiceExt;

    let config =
        rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(
            endpoint.url.clone(),
        )
        .auth_header(endpoint.token.expose().clone());
    let transport = rmcp::transport::StreamableHttpClientTransport::with_client(
        reqwest::Client::default(),
        config,
    );
    let client =
        ().serve(transport)
            .await
            .map_err(|source| RunToolCallError::Connect {
                tool: tool.to_string(),
                source: Box::new(source),
            })?;
    let mut params = rmcp::model::CallToolRequestParams::new(tool.to_string());
    if !arguments.is_empty() {
        params = params.with_arguments(arguments);
    }
    let result = client.call_tool(params).await;
    if client.cancel().await.is_err() {
        tracing::debug!("a mock session's tool client did not close");
    }
    let result = result.map_err(|source| RunToolCallError::Call {
        tool: tool.to_string(),
        source: Box::new(source),
    })?;
    answer(tool, result)
}

/// The digest of a call's answer, or the refusal the engine answered with.
fn answer(
    tool: &str,
    result: rmcp::model::CallToolResult,
) -> std::result::Result<String, RunToolCallError> {
    let text: String = result
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join(" ");
    if result.is_error.unwrap_or(false) {
        return Err(RunToolCallError::Refused {
            tool: tool.to_string(),
            text,
        });
    }
    let hash = yunta_core::sha256_hex(text.as_bytes()).to_string();
    Ok(format!("{tool}:{}", &hash[..12]))
}
