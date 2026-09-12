//! The per-run MCP server — one loopback HTTP
//! listener **per node session**, never per run: it is born just before
//! the session spawns, dies with it, and a resume always mints a fresh
//! listener and credential (a credential that survives its session is
//! reuse surface). The engine is the server, the
//! adapter translates the endpoint to its CLI's native external-MCP
//! mechanism, the agent is the client.
//!
//! **The data never lives in the listener.** Every tool reads or writes
//! the run's own storage — which is why a blackboard stays readable
//! after the join even though the listeners that posted to it are gone,
//! and why nothing here needs recovering after a crash: the listener
//! dies with the `yunta run` process it lives inside.
//!
//! **Scoping by construction.** The bearer token authenticates
//! exactly one session; the listener itself holds that session's
//! `(run_id, node_id, task)` and no tool takes a run id as a caller
//! argument — a "read me some other run" call has no surface to exist
//! on. The read restriction on the blackboard is structural too:
//! `yunta_get_blackboard` only ever serves the calling node's own posts
//! — a sibling's posts become readable only through the group's
//! post-join consolidation, never through this listener.
//!
//! **Shell edge, deliberately.** This module is the imperative shell's
//! outermost boundary — a network listener serving a live agent. The
//! high entropy the token is generated with (uuid v4) lives here and
//! only here; it participates in no pure derivation. The events a tool
//! writes, though, are the run's events like any other: they carry the
//! run's own injected [`Clock`], shared with every other emitter, so a
//! run driven by a fixed clock produces reproducible timestamps
//! throughout — the host holds an `Arc<dyn Clock>` for exactly that.

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
use yunta_core::events::{EventDraft, EventPayload, Finding, FindingPostedPayload, StoredEvent};
use yunta_core::{ArtifactSpec, Coordination, NodeId, NodeKind, RunId, TaskId, Workflow};
use yunta_storage::AsyncStorage;

use crate::observer::{append_observed, RunObserver};

/// What every listener of one run shares: its own handle on the log
/// (the listener outlives any borrow of the engine's), the run
/// identity, and which nodes sit in a `coordination: blackboard`
/// group — for anyone else, the blackboard tools are never even
/// mounted.
pub struct RunToolsHost {
    storage: AsyncStorage,
    run_id: RunId,
    blackboard_members: HashMap<NodeId, Vec<NodeId>>,
    /// Where the run keeps its artifacts. A session's working directory is
    /// the worktree, not this, so a tool that reads what the node declared
    /// has to be told.
    run_dir: PathBuf,
    /// `limits.max_artifact_bytes`, so a check and the close answer the
    /// same about a runaway file.
    max_artifact_bytes: Option<u64>,
    /// The run's injected clock — the listener stamps its own event
    /// appends with it, never a fresh `SystemClock`, so every emitter on
    /// the run shares one clock.
    clock: Arc<dyn yunta_core::Clock>,
    /// The invocation's display surface, held by the same rule as the
    /// clock and the log handle beside it: a listener outlives every
    /// borrow of the engine's, so it owns its clone. A finding an agent
    /// posts mid-session reaches a live view the moment it lands.
    observer: Option<Arc<dyn RunObserver>>,
}

impl RunToolsHost {
    pub fn new(
        storage: AsyncStorage,
        run_id: RunId,
        workflow: &Workflow,
        clock: Arc<dyn yunta_core::Clock>,
        observer: Option<Arc<dyn RunObserver>>,
        run_dir: PathBuf,
        max_artifact_bytes: Option<u64>,
    ) -> Self {
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
            clock,
            observer,
            run_dir,
            max_artifact_bytes,
        }
    }

    /// Whether `node` sits inside a `coordination: blackboard` group —
    /// the mount rule for `yunta_get_blackboard`, and the
    /// capability gate the engine checks before a session that would
    /// need it (a declared coordination the adapter can't carry is a
    /// node failure, never silent emulation).
    pub fn is_blackboard_member(&self, node: &NodeId) -> bool {
        self.blackboard_members.contains_key(node)
    }
}

/// Post-join consolidation, pure over the log: every
/// `finding_posted` authored by a member of the group, sorted by
/// `(node, finding id, title)` — **never by arrival order**, which is
/// exactly what makes two runs whose posts raced differently produce
/// byte-identical output. Written as the group's own node-output at
/// its close, consumable by a node after the `parallel`
/// (`context: [{node-output: {node: <group_id>}}]`) — never between
/// siblings hot.
pub fn consolidate_blackboard(events: &[StoredEvent], members: &[NodeId]) -> String {
    let mut entries: Vec<(String, Finding)> = events
        .iter()
        .filter_map(|event| {
            let node = event.node_id.as_ref()?;
            if !members.contains(node) {
                return None;
            }
            match event.payload() {
                Some(EventPayload::FindingPosted(p)) => Some((node.to_string(), p.finding.clone())),
                _ => None,
            }
        })
        .collect();
    entries.sort_by(|a, b| (&a.0, &a.1.id, &a.1.title).cmp(&(&b.0, &b.1.id, &b.1.title)));
    let rendered: Vec<Value> = entries
        .into_iter()
        .map(|(node, finding)| {
            let mut object = serde_json::to_value(&finding)
                .ok()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            object.insert("node".to_string(), Value::String(node));
            Value::Object(object)
        })
        .collect();
    yunta_core::yaml::to_string(&rendered).unwrap_or_default()
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
/// only ones `yunta_request_scope_expansion` exists for (scope expansion
/// is task-keyed machinery); `cwd` is where that request file lands (the
/// same worktree `scope_expansion::load_request` consumes it from).
/// What a session needs to reach the run's own tools: the host, the node
/// the listener speaks for, and the artifacts that node's close will
/// verify.
///
/// The names are already rendered, so a check inside the session and the
/// verdict at close look at the same files. A node that declares none just
/// carries an empty list — the check tool then has nothing to offer and
/// says so.
#[derive(Clone)]
pub struct RunToolsAccess {
    pub host: Arc<RunToolsHost>,
    pub node: NodeId,
    pub declared: Vec<ArtifactSpec>,
}

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
    // High entropy: 2 × 122 random bits, never logged.
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
        declared,
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
        endpoint: RunToolsEndpoint {
            url,
            token: token.into(),
        },
        shutdown,
        server,
    })
}

/// The four run tools, scoped to one session. Which of them are
/// even *listed* depends on the session: `yunta_get_blackboard` only
/// inside a `coordination: blackboard` group (for `independent`
/// they aren't mounted at all), `yunta_request_scope_expansion` only
/// for ledger-task sessions (scope expansion is task-keyed).
#[derive(Clone)]
struct SessionTools {
    host: Arc<RunToolsHost>,
    node: NodeId,
    task: Option<TaskId>,
    cwd: PathBuf,
    /// The artifacts this node's close will verify, names already
    /// rendered.
    declared: Vec<ArtifactSpec>,
}

/// Why a tool call could not be honored — rendered once, at the MCP
/// boundary, as the text the session gets back.
#[derive(Debug, thiserror::Error)]
enum RunToolError {
    #[error(
        "invalid finding — requires id, severity (blocking|major|minor|note), title, location \
         and detail: {source}"
    )]
    InvalidFinding {
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "invalid request — requires paths (list) and reason, with an optional \
         proposed_criterion {{cmd}}: {source}"
    )]
    InvalidRequest {
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "this session's node is not in a `coordination: blackboard` group — the blackboard is \
         never mounted outside one"
    )]
    NotInBlackboardGroup,
    #[error(
        "scope expansion is ledger-task machinery, keyed by task — this session has no task; a \
         prompt node's scope is fixed by its own declaration"
    )]
    NoTask,
    #[error(
        "a scope expansion request is already pending for this attempt — one request per attempt"
    )]
    RequestPending,
    #[error("the run's log cannot be reached")]
    Storage {
        #[source]
        source: yunta_storage::StorageError,
    },
    #[error("the answer cannot be rendered as JSON")]
    Render {
        #[source]
        source: serde_json::Error,
    },
    #[error("the request cannot be written as YAML")]
    Yaml {
        #[source]
        source: yunta_core::yaml::YamlError,
    },
    #[error("cannot write `{path}`")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("unknown tool `{name}`")]
    UnknownTool { name: String },
    #[error("node `{node}` declares no artifacts, so there is nothing to check")]
    NoArtifacts { node: NodeId },
    #[error("`{name}` is not an artifact this node declares; it declares {declared}")]
    UndeclaredArtifact { name: String, declared: String },
}

/// What the engine read out of an artifact, so a session sees its meaning
/// survived the parse and not only its syntax.
fn read_as(verified: &crate::artifacts::VerifiedArtifact) -> String {
    use crate::artifacts::ArtifactContent;
    match &verified.content {
        ArtifactContent::Opaque => "Verified by existence and content hash.".to_string(),
        ArtifactContent::TaskLedger(ledger) => format!(
            "{} task(s) registered: {}",
            ledger.tasks.len(),
            names(ledger.tasks.iter().map(|t| t.id.to_string()))
        ),
        ArtifactContent::Findings(findings) => format!(
            "{} finding(s) posted: {}",
            findings.len(),
            names(findings.iter().map(|f| f.id.to_string()))
        ),
        ArtifactContent::Questions(questions) => format!(
            "{} question(s) to answer: {}",
            questions.len(),
            names(questions.iter().map(|q| q.id.to_string()))
        ),
    }
}

fn names(ids: impl Iterator<Item = String>) -> String {
    let ids: Vec<String> = ids.map(|id| format!("`{id}`")).collect();
    if ids.is_empty() {
        "none".to_string()
    } else {
        ids.join(", ")
    }
}

impl SessionTools {
    fn in_blackboard_group(&self) -> bool {
        self.host.blackboard_members.contains_key(&self.node)
    }

    /// The verdict this node's close will reach, while the session can
    /// still act on it.
    ///
    /// Runs `verify_one` — the close's own verification, not a second
    /// reading of it. What comes back on success is what the engine
    /// understood, not just that the file parsed: a ledger that reads as
    /// six tasks when the session meant seven is a failure nothing else
    /// catches.
    fn check_artifact(
        &self,
        args: &serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        let wanted = args.get("name").and_then(Value::as_str);
        let specs: Vec<&ArtifactSpec> = self
            .declared
            .iter()
            .filter(|spec| wanted.is_none_or(|name| spec.name() == name))
            .collect();

        if specs.is_empty() {
            return Err(match wanted {
                Some(name) => RunToolError::UndeclaredArtifact {
                    name: name.to_string(),
                    declared: self
                        .declared
                        .iter()
                        .map(|spec| format!("`{}`", spec.name()))
                        .collect::<Vec<_>>()
                        .join(", "),
                },
                None => RunToolError::NoArtifacts {
                    node: self.node.clone(),
                },
            });
        }

        let mut verdicts = Vec::new();
        for spec in specs {
            match crate::artifacts::verify_one(
                &self.node,
                spec,
                &self.host.run_dir,
                self.host.max_artifact_bytes,
            ) {
                Ok(verified) => {
                    verdicts.push(format!("{} — ok. {}", spec.name(), read_as(&verified)))
                }
                Err(failure) => verdicts.push(
                    crate::run::repair_instruction(std::slice::from_ref(&failure))
                        .unwrap_or_else(|| failure.to_string()),
                ),
            }
        }
        Ok(verdicts.join("\n\n"))
    }

    async fn events(&self) -> Result<Vec<StoredEvent>, RunToolError> {
        self.host
            .storage
            .events_for_run(self.host.run_id.clone())
            .await
            .map_err(|source| RunToolError::Storage { source })
    }

    async fn post_finding(
        &self,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        // One schema, both intake paths: the same `Finding`
        // type the artifact path parses — an incomplete report is a
        // visible error naming the field, never free text nobody can
        // process later.
        let finding: Finding = serde_json::from_value(Value::Object(args))
            .map_err(|source| RunToolError::InvalidFinding { source })?;
        let id = finding.id.clone();
        let draft = EventDraft {
            run_id: self.host.run_id.clone(),
            node_id: Some(self.node.clone()),
            payload: EventPayload::FindingPosted(FindingPostedPayload { finding }),
        };
        append_observed(
            &self.host.storage,
            self.host.observer.as_deref(),
            draft,
            self.host.clock.now(),
        )
        .await
        .map_err(|source| RunToolError::Storage { source })?;
        Ok(format!("finding `{id}` recorded"))
    }

    async fn get_blackboard(&self) -> Result<String, RunToolError> {
        if !self.in_blackboard_group() {
            return Err(RunToolError::NotInBlackboardGroup);
        }
        // While the group runs, only this node's OWN posts —
        // reading a sibling hot would make the outcome depend on
        // arrival order, not content. Siblings' posts arrive through
        // the group's post-join consolidation, never through here.
        let own: Vec<Finding> = self
            .events()
            .await?
            .into_iter()
            .filter(|event| event.node_id.as_ref() == Some(&self.node))
            .filter_map(|event| match event.payload() {
                Some(EventPayload::FindingPosted(p)) => Some(p.finding.clone()),
                _ => None,
            })
            .collect();
        serde_json::to_string_pretty(&json!({
            "note": "your own posts only — siblings' posts become readable after the \
                     group's join, through its consolidated output",
            "findings": own,
        }))
        .map_err(|source| RunToolError::Render { source })
    }

    async fn task_status(&self) -> Result<String, RunToolError> {
        let state = crate::replay::derive(&self.events().await?);
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
        serde_json::to_string_pretty(&Value::Object(map))
            .map_err(|source| RunToolError::Render { source })
    }

    fn request_scope_expansion(
        &self,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        if self.task.is_none() {
            return Err(RunToolError::NoTask);
        }
        // The identical request object the file-based path uses,
        // validated by the same
        // type it parses — then written as that exact
        // file, so the engine's existing post-attempt evaluation
        // (rules/ask/deny, cap, findings on denial) consumes it
        // unchanged: one mechanism, two intake surfaces.
        let request: crate::scope_expansion::ScopeExpansionRequest =
            serde_json::from_value(Value::Object(args))
                .map_err(|source| RunToolError::InvalidRequest { source })?;
        let path = self
            .cwd
            .join(crate::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE);
        if path.exists() {
            return Err(RunToolError::RequestPending);
        }
        let yaml = yunta_core::yaml::to_string(&request)
            .map_err(|source| RunToolError::Yaml { source })?;
        std::fs::write(&path, yaml).map_err(|source| RunToolError::Write {
            path: path.clone(),
            source,
        })?;
        Ok(
            "request recorded — it is evaluated when this attempt ends (the engine \
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
        let mut tools = vec![
            Tool::new(
                "yunta_check_artifact",
                "Check an artifact this node declares against the contract, before your \
             session ends. Runs exactly the verification the node's close runs, so a \
             clean answer here is a clean close: write the file, call this, fix what it \
             names, call it again. On success it reports what the engine actually read \
             out of the file — so you see your meaning survived, not only your syntax. \
             Omit `name` to check every artifact this node declares.",
                object(json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "The artifact's file name, as the node declares it. \
                                            Omit to check them all."
                        }
                    }
                })),
            ),
            Tool::new(
                "yunta_post_finding",
                "Report a structured finding the moment you see it — same schema and same \
             standing as a review artifact's findings: it is counted, deduplicated \
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
            ),
        ];
        tools.push(Tool::new(
            "yunta_task_status",
            "Read-only view of the run's task ledger (task id -> status) — the same data \
             the `ledger` context source mounts, queryable mid-session.",
            object(json!({"type": "object", "properties": {}})),
        ));
        if self.task.is_some() {
            tools.push(Tool::new(
                "yunta_request_scope_expansion",
                "Ask the engine to widen this task's scope — you never widen it \
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
                 (siblings' posts become readable after the join, through the \
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
            "yunta_check_artifact" => self.check_artifact(&args),
            "yunta_post_finding" => self.post_finding(args).await,
            "yunta_get_blackboard" => self.get_blackboard().await,
            "yunta_task_status" => self.task_status().await,
            "yunta_request_scope_expansion" => self.request_scope_expansion(args),
            other => Err(RunToolError::UnknownTool {
                name: other.to_string(),
            }),
        };
        Ok(match outcome {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]).into(),
            Err(error) => {
                CallToolResult::error(vec![ContentBlock::text(yunta_core::describe(&error))]).into()
            }
        })
    }
}
