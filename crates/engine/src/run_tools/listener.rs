//! The socket one session speaks to, and the credential that is the only
//! way in.
//!
//! A listener is minted per session attempt and owned by the value it
//! returns: dropping [`RunToolsSession`] cancels the server task and the
//! port is gone with it, so no listener and no token outlives the
//! session it authenticated. The token is compared against the one
//! string this attempt was born with, which is what keeps the loopback
//! port from being a way into the run for anything else on the machine.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio_util::sync::CancellationToken;
use yunta_adapters::RunToolsEndpoint;
use yunta_core::TaskId;

use super::host::RunToolsAccess;
use super::session::SessionTools;

/// One live listener, tied to one session attempt. Dropping it tears
/// the server down — the structured-concurrency shape (the spawner owns
/// the handle; nothing is spawned and forgotten) that also guarantees
/// no listener ever outlives the session it authenticated.
pub struct RunToolsSession {
    pub endpoint: RunToolsEndpoint,
    shutdown: CancellationToken,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for RunToolsSession {
    fn drop(&mut self) {
        self.shutdown.cancel();
        self.server.abort();
    }
}

/// Starts the listener for one session attempt: fresh port, fresh
/// single-use token. `task` is `Some` for ledger-task sessions — the
/// only ones `yunta_request_scope_expansion` exists for (scope expansion
/// is task-keyed machinery); `cwd` is where that request file lands (the
/// same worktree `scope_expansion::load_request` consumes it from).
pub async fn open_session_listener(
    access: RunToolsAccess,
    task: Option<TaskId>,
    cwd: PathBuf,
) -> std::io::Result<RunToolsSession> {
    let RunToolsAccess {
        host,
        node,
        declared,
    } = access;
    let token = mint_token();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://127.0.0.1:{}/mcp", listener.local_addr()?.port());
    let shutdown = CancellationToken::new();

    let tools = SessionTools {
        host,
        node,
        task,
        cwd,
        declared,
    };
    let service = StreamableHttpService::new(
        move || Ok(tools.clone()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default().with_cancellation_token(shutdown.child_token()),
    );
    let router = behind_bearer(axum::Router::new().nest_service("/mcp", service), &token);
    let serve_ct = shutdown.clone();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async move { serve_ct.cancelled().await })
            .await;
    });

    Ok(RunToolsSession {
        endpoint: RunToolsEndpoint {
            url,
            token: token.into(),
        },
        shutdown,
        server,
    })
}

/// The credential one session attempt is reachable by.
///
/// High entropy: 2 × 122 random bits, never logged.
fn mint_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// `router`, refusing with `401` every request that does not present
/// `token`.
///
/// The gate wraps the whole router rather than the tool handlers, so a
/// request that carries no credential is answered before any session
/// state is touched — including the initialize that would otherwise open
/// a transport session of its own.
fn behind_bearer(router: axum::Router, token: &str) -> axum::Router {
    let expected = format!("Bearer {token}");
    router.layer(axum::middleware::from_fn(
        move |req: axum::extract::Request, next: axum::middleware::Next| {
            let expected = expected.clone();
            async move {
                use axum::response::IntoResponse;
                let presented = req
                    .headers()
                    .get(axum::http::header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok());
                if presented == Some(expected.as_str()) {
                    next.run(req).await
                } else {
                    axum::http::StatusCode::UNAUTHORIZED.into_response()
                }
            }
        },
    ))
}
