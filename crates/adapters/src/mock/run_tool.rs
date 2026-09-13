//! The mock's client leg: one real MCP call against the session's own
//! per-run endpoint.
//!
//! A fixture that scripts a run tool exercises the engine's listener over
//! the wire, exactly where a real CLI would place the call — the mock is
//! an adapter, not a stub of one, so what a fixture proves about the run
//! tools is what an agent would meet. What comes back is reduced to a
//! digest: the audit stream carries evidence that a call happened and
//! what it answered, never the content itself.

use crate::RunToolsEndpoint;

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
}

/// The mock's own MCP client leg: one `tools/call` against the
/// session's per-run endpoint, exactly as a real CLI would place it.
/// Returns a short digest of the response for the audit stream
/// (`ToolUse.target_digest` — never full content), or the error that
/// fails the session.
pub(super) async fn call_run_tool(
    endpoint: Option<&RunToolsEndpoint>,
    tool: &str,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> std::result::Result<String, RunToolCallError> {
    use rmcp::ServiceExt;

    let Some(endpoint) = endpoint else {
        return Err(RunToolCallError::NoEndpoint {
            tool: tool.to_string(),
        });
    };
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
    let _ = client.cancel().await;
    let result = result.map_err(|source| RunToolCallError::Call {
        tool: tool.to_string(),
        source: Box::new(source),
    })?;
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
