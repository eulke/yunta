//! `mcp:` — one query to a configured MCP server over streamable HTTP,
//! bounded by the external call timeout.

use rmcp::model::CallToolRequestParams;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::ServiceExt;
use yunta_core::Node;

use crate::template::render_template;

use super::error::ContextResolveError;
use super::EXTERNAL_CALL_TIMEOUT;
use crate::run::node_exec::template_vars;
use crate::run::RunCtx;

/// `mcp: { server, query }`: looks `server` up in the merged
/// config's `mcp_servers:`, connects over streamable-HTTP (bearer token
/// read from the env var `auth_env` names, never from config itself),
/// and calls a tool literally named `query` with the rendered `query:`
/// text as its sole argument — see the module doc for why that specific
/// mapping. The whole round trip (connect, handshake, call) is bounded
/// by `EXTERNAL_CALL_TIMEOUT`; the connection is always closed before
/// returning, success or failure alike.
pub(super) async fn resolve_mcp(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    params: &yunta_core::McpQueryParams,
) -> Result<Vec<u8>, ContextResolveError> {
    let server = ctx
        .manifest
        .config
        .mcp_servers
        .as_ref()
        .and_then(|servers| servers.get(&params.server))
        .ok_or_else(|| ContextResolveError::UnknownMcpServer {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            server: params.server.clone(),
        })?;

    let vars = template_vars(ctx, node);
    let query =
        render_template(&params.query, &vars).map_err(|e| ContextResolveError::Template {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            source: e,
        })?;

    let auth_header = match &server.auth_env {
        Some(var) => Some(
            std::env::var(var).map_err(|_| ContextResolveError::MissingAuthEnv {
                node: node.id.clone(),
                source_id: source_id.to_string(),
                server: params.server.clone(),
                var: var.clone(),
            })?,
        ),
        None => None,
    };

    let call = call_mcp_query(server.url.clone(), auth_header, query);
    let text = tokio::time::timeout(EXTERNAL_CALL_TIMEOUT, call)
        .await
        .map_err(|_elapsed| ContextResolveError::McpTimedOut {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            server: params.server.clone(),
        })?
        .map_err(|source| ContextResolveError::McpFailed {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            server: params.server.clone(),
            source,
        })?;
    Ok(text.into_bytes())
}

/// Why one `query` call to an MCP server yielded no text.
#[derive(Debug, thiserror::Error)]
pub(in crate::run) enum McpQueryError {
    #[error("cannot connect")]
    Connect {
        #[source]
        source: Box<rmcp::service::ClientInitializeError>,
    },
    #[error("the `query` call failed")]
    Call {
        #[source]
        source: Box<rmcp::service::ServiceError>,
    },
    #[error("tool `query` returned an error: {text}")]
    Refused { text: String },
    #[error("unexpected tools/call response: {response}")]
    Unexpected { response: String },
}

/// Connects, calls the `query` tool once, and disconnects — isolated from
/// `resolve_mcp` so the rmcp plumbing meets `ContextResolveError` as one
/// typed cause.
async fn call_mcp_query(
    url: String,
    auth_header: Option<String>,
    query: String,
) -> Result<String, McpQueryError> {
    let mut config = StreamableHttpClientTransportConfig::with_uri(url);
    if let Some(token) = auth_header {
        config = config.auth_header(token);
    }
    let transport = StreamableHttpClientTransport::with_client(reqwest::Client::default(), config);
    let client =
        ().serve(transport)
            .await
            .map_err(|source| McpQueryError::Connect {
                source: Box::new(source),
            })?;

    let mut arguments = rmcp::model::JsonObject::new();
    arguments.insert("query".to_string(), serde_json::Value::String(query));
    let result = client
        .call_tool_once(CallToolRequestParams::new("query").with_arguments(arguments))
        .await;
    let _ = client.cancel().await;

    match result.map_err(|source| McpQueryError::Call {
        source: Box::new(source),
    })? {
        rmcp::model::CallToolResponse::Complete(result) => {
            let text: String = result
                .content
                .into_iter()
                .filter_map(|block| match block {
                    rmcp::model::ContentBlock::Text(t) => Some(t.text),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if result.is_error == Some(true) {
                Err(McpQueryError::Refused { text })
            } else {
                Ok(text)
            }
        }
        other => Err(McpQueryError::Unexpected {
            response: format!("{other:?}"),
        }),
    }
}
