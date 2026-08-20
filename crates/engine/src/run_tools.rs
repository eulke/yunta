//! The per-run MCP server (§6.4/§6.5, D103, T8.2) — one loopback HTTP
//! listener **per node session**, never per run: it is born just before
//! the session spawns, dies with it, and a resume always mints a fresh
//! listener and credential (a credential that survives its session is
//! reuse surface, §6.5's own words). The engine is the server, the
//! adapter translates the endpoint to its CLI's native external-MCP
//! mechanism, the agent is the client.
//!
//! **The data never lives in the listener.** Every tool reads or writes
//! the run's own storage — which is why a blackboard stays readable
//! after the join even though the listeners that posted to it are gone,
//! and why nothing here needs recovering after a crash: the listener
//! dies with the `yunta run` process it lives inside.
//!
//! **Scoping by construction (I27).** The bearer token authenticates
//! exactly one session; the listener itself holds that session's
//! `(run_id, node_id, task)` and no tool takes a run id as a caller
//! argument — a "read me some other run" call has no surface to exist
//! on. §5.9/D98's read restriction is structural too:
//! `yunta_get_blackboard` only ever serves the calling node's own posts
//! — a sibling's posts become readable only through the group's
//! post-join consolidation, never through this listener.
//!
//! **Shell edge, deliberately.** This module is the imperative shell's
//! outermost boundary — a network listener serving a live agent. The
//! entropy D103 mandates for the token (uuid v4) and the wall-clock
//! timestamps on tool-written events both live here and only here:
//! neither participates in any pure derivation (replay consumes stored
//! timestamps), and injecting them would thread `Arc`s through every
//! `execute_run` caller for no reproducibility gain — the moment an
//! agent calls a tool is real wall time by nature, exactly like the
//! session audit events surrounding it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use yunta_adapters::RunToolsEndpoint;
use yunta_core::events::{Event, EventPayload, Finding, FindingPostedPayload};
use yunta_core::{Coordination, NodeId, NodeKind, RunId, TaskId, Workflow};
use yunta_storage::Storage;

/// What every listener of one run shares: its own storage handle (a
/// [`Storage::reopen`]ed one — the listener outlives any borrow of the
/// engine's), the run identity, and which nodes sit in a
/// `coordination: blackboard` group (D49: for anyone else, the
/// blackboard tools are never even mounted).
pub struct RunToolsHost {
    storage: Storage,
    run_id: RunId,
    blackboard_members: HashMap<NodeId, Vec<NodeId>>,
}

impl RunToolsHost {
    pub fn new(storage: Storage, run_id: RunId, workflow: &Workflow) -> Self {
        let mut blackboard_members = HashMap::new();
        for node in &workflow.nodes {
            if let NodeKind::Parallel {
                coordination: Coordination::Blackboard,
                nodes: children,
                ..
            } = &node.kind
            {
                let member_ids: Vec<NodeId> = children.iter().map(|c| c.id.clone()).collect();
                for child in children {
                    blackboard_members.insert(child.id.clone(), member_ids.clone());
                }
            }
        }
        Self {
            storage,
            run_id,
            blackboard_members,
        }
    }
}

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
/// only ones `yunta_request_scope_expansion` exists for (§6.2 is
/// task-keyed machinery); `cwd` is where that request file lands (the
/// same worktree `scope_expansion::load_request` consumes it from).
pub async fn open_session_listener(
    host: Arc<RunToolsHost>,
    node: NodeId,
    task: Option<TaskId>,
    cwd: PathBuf,
) -> std::io::Result<RunToolsSession> {
    // D103's "alta entropía": 2 × 122 random bits, never logged (I12).
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://127.0.0.1:{}/mcp", listener.local_addr()?.port());
    let shutdown = CancellationToken::new();

    let tools = SessionTools {
        host,
        node,
        task,
        cwd,
    };
    let service = StreamableHttpService::new(
        move || Ok(tools.clone()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default().with_cancellation_token(shutdown.child_token()),
    );
    let expected = format!("Bearer {token}");
    let router =
        axum::Router::new()
            .nest_service("/mcp", service)
            .layer(axum::middleware::from_fn(
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
            ));
    let serve_ct = shutdown.clone();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async move { serve_ct.cancelled().await })
            .await;
    });

    Ok(RunToolsSession {
        endpoint: RunToolsEndpoint { url, token },
        shutdown,
        server,
    })
}

/// The four tools of §6.4, scoped to one session. Which of them are
/// even *listed* depends on the session: `yunta_get_blackboard` only
/// inside a `coordination: blackboard` group (D49 — for `independent`
/// they aren't mounted at all), `yunta_request_scope_expansion` only
/// for ledger-task sessions (§6.2 is task-keyed).
#[derive(Clone)]
struct SessionTools {
    host: Arc<RunToolsHost>,
    node: NodeId,
    task: Option<TaskId>,
    cwd: PathBuf,
}

impl SessionTools {
    fn in_blackboard_group(&self) -> bool {
        self.host.blackboard_members.contains_key(&self.node)
    }

    fn events(&self) -> Result<Vec<Event>, String> {
        self.host
            .storage
            .events_for_run(&self.host.run_id)
            .map_err(|e| e.to_string())
    }

    fn post_finding(&self, args: serde_json::Map<String, Value>) -> Result<String, String> {
        // §4.1's one schema, both intake paths (D80): the same `Finding`
        // type the artifact path parses — an incomplete report is a
        // visible error naming the field, never free text nobody can
        // process later.
        let finding: Finding = serde_json::from_value(Value::Object(args))
            .map_err(|e| format!("invalid finding — §4.1 requires id, severity (blocking|major|minor|note), title, location and detail: {e}"))?;
        let id = finding.id.clone();
        self.host
            .storage
            .append_event(&Event {
                run_id: self.host.run_id.clone(),
                seq: 0,
                timestamp: chrono::Utc::now(),
                node_id: Some(self.node.clone()),
                payload: EventPayload::FindingPosted(FindingPostedPayload { finding }),
            })
            .map_err(|e| e.to_string())?;
        Ok(format!("finding `{id}` recorded"))
    }

    fn get_blackboard(&self) -> Result<String, String> {
        if !self.in_blackboard_group() {
            return Err(
                "this session's node is not in a `coordination: blackboard` group — the \
                 blackboard is never mounted outside one (D49)"
                    .to_string(),
            );
        }
        // §5.9/D98: while the group runs, only this node's OWN posts —
        // reading a sibling hot would make the outcome depend on
        // arrival order, not content. Siblings' posts arrive through
        // the group's post-join consolidation, never through here.
        let own: Vec<Finding> = self
            .events()?
            .into_iter()
            .filter(|event| event.node_id.as_ref() == Some(&self.node))
            .filter_map(|event| match event.payload {
                EventPayload::FindingPosted(p) => Some(p.finding),
                _ => None,
            })
            .collect();
        serde_json::to_string_pretty(&json!({
            "note": "your own posts only — siblings' posts become readable after the \
                     group's join, through its consolidated output (§5.9)",
            "findings": own,
        }))
        .map_err(|e| e.to_string())
    }

    fn task_status(&self) -> Result<String, String> {
        let state = crate::replay::derive(&self.events()?);
        let mut tasks: Vec<(String, String)> = state
            .tasks
            .iter()
            .map(|(id, status)| {
                (
                    id.to_string(),
                    serde_json::to_value(status)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_else(|| format!("{status:?}")),
                )
            })
            .collect();
        tasks.sort();
        let map: serde_json::Map<String, Value> = tasks
            .into_iter()
            .map(|(id, status)| (id, Value::String(status)))
            .collect();
        serde_json::to_string_pretty(&Value::Object(map)).map_err(|e| e.to_string())
    }

    fn request_scope_expansion(
        &self,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, String> {
        if self.task.is_none() {
            return Err(
                "scope expansion is ledger-task machinery (§6.2, keyed by task) — this \
                 session has no task; a prompt node's scope is fixed by its own declaration"
                    .to_string(),
            );
        }
        // The identical request object of §6.2, validated by the same
        // type the file-based path parses — then written as that exact
        // file, so the engine's existing post-attempt evaluation
        // (rules/ask/deny, cap, findings on denial) consumes it
        // unchanged: one mechanism, two intake surfaces.
        let request: crate::scope_expansion::ScopeExpansionRequest =
            serde_json::from_value(Value::Object(args)).map_err(|e| {
                format!(
                    "invalid request — §6.2 requires paths (list) and reason, with an \
                     optional proposed_criterion {{cmd}}: {e}"
                )
            })?;
        let path = self
            .cwd
            .join(crate::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE);
        if path.exists() {
            return Err(
                "a scope expansion request is already pending for this attempt — one \
                 request per attempt (§6.2)"
                    .to_string(),
            );
        }
        let yaml = serde_yaml::to_string(&request).map_err(|e| e.to_string())?;
        std::fs::write(&path, yaml).map_err(|e| e.to_string())?;
        Ok(
            "request recorded — it is evaluated when this attempt ends (§6.2: the engine \
             or a person decides; a denial becomes a finding); re-attempt the work after"
                .to_string(),
        )
    }
}

impl ServerHandler for SessionTools {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let object = |schema: Value| schema.as_object().cloned().unwrap_or_default();
        let mut tools = vec![Tool::new(
            "yunta_post_finding",
            "Report a structured finding the moment you see it — same schema and same \
             standing as a review artifact's findings (§4.1): it is counted, deduplicated \
             and consulted with them, and survives this session.",
            object(json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "severity": {"type": "string", "enum": ["blocking", "major", "minor", "note"]},
                    "title": {"type": "string"},
                    "location": {"type": "string", "description": "path, optionally with a range"},
                    "detail": {"type": "string"},
                    "proposed_criterion": {"type": "object", "properties": {"cmd": {"type": "string"}}, "required": ["cmd"]},
                },
                "required": ["id", "severity", "title", "location", "detail"],
            })),
        )];
        tools.push(Tool::new(
            "yunta_task_status",
            "Read-only view of the run's task ledger (task id -> status) — the same data \
             the `ledger` context source mounts, queryable mid-session.",
            object(json!({"type": "object", "properties": {}})),
        ));
        if self.task.is_some() {
            tools.push(Tool::new(
                "yunta_request_scope_expansion",
                "Ask the engine to widen this task's scope (§6.2) — you never widen it \
                 yourself. Provide the paths, the reason, and a verifiable criterion that \
                 is red today; the request is evaluated when this attempt ends, and a \
                 denial becomes a finding rather than silence.",
                object(json!({
                    "type": "object",
                    "properties": {
                        "paths": {"type": "array", "items": {"type": "string"}},
                        "reason": {"type": "string"},
                        "proposed_criterion": {"type": "object", "properties": {"cmd": {"type": "string"}}, "required": ["cmd"]},
                    },
                    "required": ["paths", "reason"],
                })),
            ));
        }
        if self.in_blackboard_group() {
            tools.push(Tool::new(
                "yunta_get_blackboard",
                "Read this group's blackboard — your OWN posts only while the group runs \
                 (§5.9: siblings' posts become readable after the join, through the \
                 group's consolidated output, so results never depend on arrival order).",
                object(json!({"type": "object", "properties": {}})),
            ));
        }
        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, McpError> {
        let args = request.arguments.unwrap_or_default();
        let outcome = match request.name.as_ref() {
            "yunta_post_finding" => self.post_finding(args),
            "yunta_get_blackboard" => self.get_blackboard(),
            "yunta_task_status" => self.task_status(),
            "yunta_request_scope_expansion" => self.request_scope_expansion(args),
            other => Err(format!("unknown tool `{other}`")),
        };
        Ok(match outcome {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]).into(),
            Err(text) => CallToolResult::error(vec![ContentBlock::text(text)]).into(),
        })
    }
}
