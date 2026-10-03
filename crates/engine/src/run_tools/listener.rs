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

use super::host::{NodeScopeAccess, RunToolsAccess, TaskAccess};
use super::session::SessionTools;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio_util::sync::CancellationToken;
use yunta_core::port::RunToolsEndpoint;
use yunta_core::RunTool;

/// One live listener, tied to one session attempt. Dropping it tears
/// the server down — the structured-concurrency shape (the spawner owns
/// the handle; nothing is spawned and forgotten) that also guarantees
/// no listener ever outlives the session it authenticated.
pub struct RunToolsSession {
    pub endpoint: RunToolsEndpoint,
    /// How the session's CLI names these tools to its model.
    naming: yunta_core::ToolNaming,
    /// The tools this session is served, in the order it reads them.
    offered: Vec<RunTool>,
    shutdown: CancellationToken,
    server: tokio::task::JoinHandle<()>,
}

impl RunToolsSession {
    /// How this session's CLI names these tools to its model.
    pub fn naming(&self) -> yunta_core::ToolNaming {
        self.naming
    }

    /// What this session's model calls `tool` by — the one name every
    /// text it is shown gives that tool.
    pub fn called(&self, tool: RunTool) -> String {
        tool.called(self.naming)
    }

    /// Each tool this session is served, as its model calls it, beside
    /// the tool's own name — empty when the two are the same.
    pub(crate) fn renamed(&self) -> Vec<(String, &'static str)> {
        match self.naming {
            yunta_core::ToolNaming::Bare => Vec::new(),
            _ => self
                .offered
                .iter()
                .map(|tool| (self.called(*tool), tool.name()))
                .collect(),
        }
    }
}

impl Drop for RunToolsSession {
    fn drop(&mut self) {
        self.shutdown.cancel();
        self.server.abort();
    }
}

/// Starts the listener for one session attempt: fresh port, fresh
/// single-use token. `task` is `Some` for task sessions — the only ones
/// the task tools exist for; `node_scope` is `Some` for a node's own
/// session when the node declares a scope, which is what `yunta_check_scope`
/// judges. Either can ask with `yunta_request_scope_expansion`; `cwd` is
/// where a request file lands (the same worktree
/// `scope_expansion::load_request` consumes it from).
pub async fn open_session_listener(
    access: RunToolsAccess,
    (task, node_scope): (Option<Arc<TaskAccess>>, Option<Arc<NodeScopeAccess>>),
    cwd: PathBuf,
) -> std::io::Result<RunToolsSession> {
    let token = mint_token();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://127.0.0.1:{}/mcp", listener.local_addr()?.port());
    // What a tool runs for a task session stops with the task, and with the listener.
    let shutdown = task
        .as_ref()
        .map_or_else(CancellationToken::new, |t| t.cancel.child_token());

    let naming = access.naming;
    let tools = SessionTools::new(access, (task, node_scope), cwd, shutdown.clone());
    let offered = super::catalog::offered(&tools);
    let service = StreamableHttpService::new(
        move || Ok(tools.clone()),
        Arc::new(transport_sessions()),
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
        naming,
        offered,
        shutdown,
        server,
    })
}

/// The transport sessions one listener keeps, none of which is ever
/// closed for being quiet.
///
/// A transport session only hears something when a message passes
/// through it, and a tool that runs a task's criteria sends nothing
/// until it answers. Closed after a quiet spell, the session would drop
/// that answer while the client went on waiting for it. What the idle
/// limit is otherwise for — a session nobody will speak to again — the
/// listener already ends: it dies with the session attempt it serves.
fn transport_sessions() -> LocalSessionManager {
    let mut sessions = LocalSessionManager::default();
    sessions.session_config.keep_alive = None;
    sessions
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
    let expected = yunta_core::Secret::from(format!("Bearer {token}"));
    router.layer(axum::middleware::from_fn(
        move |req: axum::extract::Request, next: axum::middleware::Next| {
            let expected = expected.clone();
            async move {
                use axum::response::IntoResponse;
                let presented = req
                    .headers()
                    .get(axum::http::header::AUTHORIZATION)
                    .map(axum::http::HeaderValue::as_bytes)
                    .unwrap_or_default();
                if presents_the_credential(presented, &expected) {
                    next.run(req).await
                } else {
                    axum::http::StatusCode::UNAUTHORIZED.into_response()
                }
            }
        },
    ))
}

/// Whether `presented` is the credential, compared in time that does
/// not depend on how much of it is right.
///
/// A `==` on two strings stops at the first differing byte, so a caller
/// that can time the answer learns how long a prefix it guessed and can
/// walk the whole credential out one byte at a time. The lengths are
/// compared first and in the clear, which says only how long the
/// credential is — a constant of this build, not a secret.
fn presents_the_credential(presented: &[u8], expected: &yunta_core::Secret<String>) -> bool {
    use subtle::ConstantTimeEq;
    let expected = expected.expose().as_bytes();
    presented.len() == expected.len() && bool::from(presented.ct_eq(expected))
}

#[cfg(test)]
mod quiet_session_tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use rmcp::model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ServerCapabilities,
        ServerInfo,
    };
    use rmcp::service::{RequestContext, RunningService};
    use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
    use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService,
    };
    use rmcp::transport::StreamableHttpClientTransport;
    use rmcp::{ErrorData, RoleClient, RoleServer, ServerHandler, ServiceExt};
    use tokio::sync::Notify;

    /// A tool that sends nothing until the test lets it answer — the
    /// shape of a check running a suite.
    #[derive(Clone, Default)]
    struct Held {
        entered: Arc<AtomicBool>,
        release: Arc<Notify>,
    }

    impl ServerHandler for Held {
        fn get_info(&self) -> ServerInfo {
            ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
        }

        async fn call_tool(
            &self,
            _request: CallToolRequestParams,
            _context: RequestContext<RoleServer>,
        ) -> Result<CallToolResponse, ErrorData> {
            self.entered.store(true, Ordering::SeqCst);
            self.release.notified().await;
            Ok(CallToolResult::success(vec![ContentBlock::text("done")]).into())
        }
    }

    /// A client connected to a server whose transport sessions are
    /// `sessions`, and whose one tool is `held`.
    async fn serve(
        sessions: Arc<LocalSessionManager>,
        held: Held,
    ) -> RunningService<RoleClient, ()> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://127.0.0.1:{}/mcp",
            listener.local_addr().unwrap().port()
        );
        let service = StreamableHttpService::new(
            move || Ok(held.clone()),
            sessions,
            StreamableHttpServerConfig::default(),
        );
        tokio::spawn(async move {
            let _ = axum::serve(listener, axum::Router::new().nest_service("/mcp", service)).await;
        });
        let transport = StreamableHttpClientTransport::with_client(
            reqwest::Client::default(),
            StreamableHttpClientTransportConfig::with_uri(url),
        );
        ().serve(transport).await.unwrap()
    }

    /// Starts one call on `client`, and returns once `held` is running it.
    async fn call_held(
        client: Arc<RunningService<RoleClient, ()>>,
        held: &Held,
    ) -> tokio::task::JoinHandle<bool> {
        let call = tokio::spawn(async move {
            client
                .call_tool(CallToolRequestParams::new("held".to_string()))
                .await
                .is_ok()
        });
        let entered = held.entered.clone();
        yunta_testkit::wait_until_async(
            || {
                let entered = entered.clone();
                async move { entered.load(Ordering::SeqCst) }
            },
            || "the call never reached the tool".to_string(),
        )
        .await;
        call
    }

    #[tokio::test]
    async fn a_session_closed_for_being_quiet_drops_the_answer_of_a_call_still_running() {
        let mut closes_when_quiet = LocalSessionManager::default();
        closes_when_quiet.session_config.keep_alive = Some(Duration::from_millis(200));
        let sessions = Arc::new(closes_when_quiet);
        let held = Held::default();
        let client = Arc::new(serve(sessions.clone(), held.clone()).await);
        let call = call_held(client, &held).await;

        yunta_testkit::wait_until_async(
            || {
                let sessions = sessions.clone();
                async move { sessions.sessions.read().await.is_empty() }
            },
            || "the quiet session was never closed".to_string(),
        )
        .await;
        held.release.notify_one();

        let answered = tokio::time::timeout(Duration::from_secs(2), call).await;
        assert!(
            !matches!(answered, Ok(Ok(true))),
            "the tool answered after its session closed, and the answer went nowhere"
        );
    }

    #[tokio::test]
    async fn the_listener_s_sessions_are_never_closed_for_being_quiet() {
        assert_eq!(super::transport_sessions().session_config.keep_alive, None);
        let held = Held::default();
        let client = Arc::new(serve(Arc::new(super::transport_sessions()), held.clone()).await);
        let call = call_held(client, &held).await;
        held.release.notify_one();
        assert!(call.await.unwrap(), "a held call answers once it is let go");
    }
}

#[cfg(test)]
mod bearer_tests {
    use super::presents_the_credential;
    use yunta_core::Secret;

    /// The comparison is constant time, which no test can assert by
    /// timing. What a test can hold is the shape that makes it so: the
    /// expected value stays wrapped, and the bytes go through
    /// `subtle::ConstantTimeEq` rather than `==`. A rewrite back to
    /// `==` would not compile against a `Secret<String>` without first
    /// exposing it, which is the line this keeps visible.
    #[test]
    fn the_bearer_check_is_constant_time() {
        let expected = Secret::from("Bearer abcdef".to_string());

        assert!(presents_the_credential(b"Bearer abcdef", &expected));
        assert!(!presents_the_credential(b"Bearer abcdeg", &expected));
        assert!(
            !presents_the_credential(b"Bearer abcde", &expected),
            "a prefix is not the credential"
        );
        assert!(
            !presents_the_credential(b"Bearer abcdefg", &expected),
            "and neither is something longer that starts with it"
        );
        assert!(!presents_the_credential(b"", &expected));
    }
}
