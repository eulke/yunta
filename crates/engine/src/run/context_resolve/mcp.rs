//! `mcp:` — one query to a configured MCP server over streamable HTTP,
//! bounded by the external call timeout.

use std::future::Future;

use rmcp::model::CallToolRequestParams;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::ServiceExt;
use yunta_core::best_effort::{self, BestEffortStep, Shortfall};
use yunta_core::events::{BestEffortFailedPayload, EventPayload, SessionEvent};
use yunta_core::{McpServerName, Node};

use yunta_core::template::render_template;

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
        Some(var) => {
            let missing = || ContextResolveError::MissingAuthEnv {
                node: node.id.clone(),
                source_id: source_id.to_string(),
                server: params.server.clone(),
                var: var.clone(),
            };
            Some(ctx.secrets.get(var).ok_or_else(missing)?)
        }
        None => None,
    };

    let call = call_mcp_query(server.url.clone(), auth_header, query);
    let text = answer_of(ctx, node, source_id, &params.server, call).await?;
    Ok(text.into_bytes())
}

/// The answer of the query `call` makes, bounded by the external call
/// timeout. The close that follows the query is recorded ahead of the answer
/// decides the node's outcome: a close that failed stays on the log beside
/// the answer, and never takes its place.
async fn answer_of(
    ctx: &RunCtx<'_>,
    node: &Node,
    source_id: &str,
    server: &McpServerName,
    call: impl Future<Output = (Result<String, McpQueryError>, Option<Shortfall>)>,
) -> Result<String, ContextResolveError> {
    let (answer, closed) = tokio::time::timeout(EXTERNAL_CALL_TIMEOUT, call)
        .await
        .map_err(|_elapsed| ContextResolveError::McpTimedOut {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            server: server.clone(),
        })?;
    if let Some(shortfall) = closed {
        ctx.emit(
            Some(&node.id),
            EventPayload::Session(SessionEvent::BestEffortFailed(BestEffortFailedPayload {
                step: shortfall.step,
                cause: shortfall.cause,
            })),
        )
        .await
        .map_err(|source| ContextResolveError::Io {
            node: node.id.clone(),
            source_id: source_id.to_string(),
            action: "record best_effort_failed".to_string(),
            source: std::io::Error::other(source.to_string()),
        })?;
    }
    answer.map_err(|source| ContextResolveError::McpFailed {
        node: node.id.clone(),
        source_id: source_id.to_string(),
        server: server.clone(),
        source,
    })
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
    #[error("tool `query` gave no answer: {response}")]
    Unexpected { response: String },
}

/// Connects, calls the `query` tool once, and disconnects — isolated from
/// `resolve_mcp` so the rmcp plumbing meets `ContextResolveError` as one
/// typed cause. The close is attempted after the call whatever it answered,
/// and a close that fails comes back beside the answer as a shortfall.
/// The bearer stays wrapped until the transport takes it: it is handed
/// to one config field and never to a log, a message or a `Debug`.
async fn call_mcp_query(
    url: String,
    auth_header: Option<yunta_core::Secret<String>>,
    query: String,
) -> (Result<String, McpQueryError>, Option<Shortfall>) {
    let mut config = StreamableHttpClientTransportConfig::with_uri(url);
    if let Some(token) = auth_header {
        config = config.auth_header(token.expose().to_string());
    }
    let transport = StreamableHttpClientTransport::with_client(reqwest::Client::default(), config);
    let client = match ().serve(transport).await {
        Ok(client) => client,
        Err(source) => {
            let connect = McpQueryError::Connect {
                source: Box::new(source),
            };
            return (Err(connect), None);
        }
    };

    let mut arguments = rmcp::model::JsonObject::new();
    arguments.insert("query".to_string(), serde_json::Value::String(query));
    let result = client
        .call_tool_once(CallToolRequestParams::new("query").with_arguments(arguments))
        .await;
    let closed = client
        .cancel()
        .await
        .err()
        .map(|error| best_effort::record(BestEffortStep::McpClientClose, error));
    let answer = match result {
        Ok(reply) => answer_text(reply),
        Err(source) => Err(McpQueryError::Call {
            source: Box::new(source),
        }),
    };
    (answer, closed)
}

/// The text a `query` answer carries, or why it carries none.
fn answer_text(reply: rmcp::model::CallToolResponse) -> Result<String, McpQueryError> {
    match reply {
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
        rmcp::model::CallToolResponse::InputRequired(_) => Err(McpQueryError::Unexpected {
            response: "it asked for input before answering, and a context query has none to give"
                .to_string(),
        }),
        rmcp::model::CallToolResponse::Task(_) => Err(McpQueryError::Unexpected {
            response: "it answered with a task to poll, and a context query does not wait on one"
                .to_string(),
        }),
        _ => Err(McpQueryError::Unexpected {
            response: "it answered with a kind of response this binary does not know".to_string(),
        }),
    }
}
