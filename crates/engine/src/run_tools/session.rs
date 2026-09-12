//! The tool surface of one session: what every tool is scoped to, how a
//! call reaches the handler that answers it, and how a failure becomes
//! text the agent can act on.
//!
//! The scope is the struct itself. A handler reads the node, the task and
//! the working directory from here, never from the call, so no tool has a
//! surface on which to name another run, another node or another
//! session's worktree. The log is reached through this one pair of
//! methods for the same reason: every tool that records something records
//! it against this session's run and node, stamped with the run's own
//! clock.
//!
//! Failure text is rendered exactly once, here at the boundary: a handler
//! returns a typed [`RunToolError`] and `call_tool` turns it into the
//! content block the agent reads.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ListToolsResult, PaginatedRequestParams,
};
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use yunta_core::events::{EventDraft, EventPayload, StoredEvent};
use yunta_core::{ArtifactKind, ArtifactSpec, NodeId, TaskId};

use super::host::RunToolsHost;

/// The run tools of one session. Which of them are even *listed* depends
/// on the session: `yunta_get_blackboard` only inside a
/// `coordination: blackboard` group (for `independent` they aren't
/// mounted at all), `yunta_request_scope_expansion` only for ledger-task
/// sessions (scope expansion is task-keyed), and one submission tool per
/// kind of document this node declares.
#[derive(Clone)]
pub(super) struct SessionTools {
    pub(super) host: Arc<RunToolsHost>,
    pub(super) node: NodeId,
    pub(super) task: Option<TaskId>,
    pub(super) cwd: PathBuf,
    /// The artifacts this node's close will verify, names already
    /// rendered.
    pub(super) declared: Vec<ArtifactSpec>,
}

/// Why a tool call could not be honored — rendered once, at the MCP
/// boundary, as the text the session gets back.
#[derive(Debug, thiserror::Error)]
pub(super) enum RunToolError {
    /// A refusal a session acts on: the numbered problems to fix, in the
    /// words the document's own diagnostics use. Never a deserializer's
    /// account of itself.
    #[error("{text}")]
    Refused { text: String },
    #[error("invalid submission — requires `name` (one of {names}) and `document` (an object): {detail}")]
    InvalidSubmission { names: String, detail: String },
    #[error(
        "`{name}` is declared by this node with kind `{declared}`, not the kind this tool \
         submits — use `{expected}`"
    )]
    WrongKind {
        name: String,
        declared: String,
        expected: String,
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

impl SessionTools {
    /// The run's events, as every tool that derives state reads them.
    pub(super) async fn events(&self) -> Result<Vec<StoredEvent>, RunToolError> {
        self.host
            .storage
            .events_for_run(self.host.run_id.clone())
            .await
            .map_err(|source| RunToolError::Storage { source })
    }

    /// Records `payload` against this session's run and node, stamped
    /// with the run's own clock.
    pub(super) async fn append(&self, payload: EventPayload) -> Result<(), RunToolError> {
        self.host
            .storage
            .append(
                EventDraft {
                    run_id: self.host.run_id.clone(),
                    node_id: Some(self.node.clone()),
                    payload,
                },
                self.host.clock.now(),
            )
            .await
            .map(|_| ())
            .map_err(|source| RunToolError::Storage { source })
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
        Ok(ListToolsResult {
            tools: super::catalog::mounted(self),
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
            name if name == ArtifactKind::UPDATE_FINDING_TOOL => self.update_finding(args).await,
            name if name == ArtifactKind::WITHDRAW_FINDING_TOOL => {
                self.withdraw_finding(args).await
            }
            "yunta_get_blackboard" => self.get_blackboard().await,
            "yunta_task_status" => self.task_status().await,
            "yunta_request_scope_expansion" => self.request_scope_expansion(args),
            // A submission tool names its own kind, so the name that
            // matched is the kind that answers it.
            other => match ArtifactKind::from_submit_tool(other) {
                Some(kind) => self.submit(kind, args).await,
                None => Err(RunToolError::UnknownTool {
                    name: other.to_string(),
                }),
            },
        };
        Ok(match outcome {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]).into(),
            Err(error) => {
                CallToolResult::error(vec![ContentBlock::text(yunta_core::describe(&error))]).into()
            }
        })
    }
}
