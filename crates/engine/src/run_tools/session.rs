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

use super::catalog::RunTool;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ListToolsResult, PaginatedRequestParams,
};
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use tokio_util::sync::CancellationToken;
use yunta_core::events::{EventPayload, StoredEvent};
use yunta_core::{ArtifactSpec, NodeId, NodeKind};

use super::host::{RunToolsHost, TaskAccess};
use crate::run_log::RunLog;

/// The run tools of one session. Which of them are even *listed* depends
/// on the session: `yunta_get_blackboard` only inside a
/// `coordination: blackboard` group (for `independent` they aren't
/// mounted at all), `yunta_request_scope_expansion` only for task
/// sessions (scope expansion is task-keyed), and one submission tool per
/// kind of document this node declares.
#[derive(Clone)]
pub(super) struct SessionTools {
    pub(super) host: Arc<RunToolsHost>,
    pub(super) node: NodeId,
    /// What the node is, so a verdict asks who answers for an artifact
    /// the same way its close does.
    pub(super) node_kind: NodeKind,
    /// The task this session works, for a loop's task session: what
    /// the task tools read and judge, and what makes this a session a
    /// scope expansion can be asked for.
    pub(super) task: Option<Arc<TaskAccess>>,
    pub(super) cwd: PathBuf,
    /// What stops a command a tool runs for this session: the task's own
    /// token, and the session's end.
    pub(super) stop: CancellationToken,
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
    #[error(
        "invalid submission — requires `document` (an object), the {names} document this node \
         declares: {detail}"
    )]
    InvalidSubmission { names: String, detail: String },
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
        "scope expansion is task machinery, keyed by task — this session has no task; a \
         prompt node's scope is fixed by its own declaration"
    )]
    NoTask,
    #[error(
        "a scope expansion request is already pending for this attempt — one request per attempt"
    )]
    RequestPending,
    #[error(
        "`{tool}` answers about a task, and this session works none — only a loop's task \
         sessions are served it"
    )]
    NotATaskSession { tool: &'static str },
    #[error("the task's work could not be checked")]
    Check {
        #[source]
        source: crate::task_cycle::TaskCycleError,
    },
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
    #[error(transparent)]
    Accept {
        #[from]
        source: crate::artifacts::AcceptError,
    },
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

    /// The run's log as this listener reaches it: the host's own handle,
    /// the run's identity and its clock.
    pub(super) fn log(&self) -> RunLog<'_> {
        RunLog::new(
            &self.host.storage,
            &self.host.run_id,
            self.host.clock.as_ref(),
            &self.host.redactor,
        )
        .observed_by(self.host.observer.as_deref())
    }

    /// Records `payload` against this session's run and node, stamped
    /// with the run's own clock.
    pub(super) async fn append(&self, payload: EventPayload) -> Result<(), RunToolError> {
        self.log()
            .record(Some(&self.node), payload)
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
        Ok(crate::mcp::tool_list(super::catalog::mounted(self)))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, McpError> {
        let args = request.arguments.unwrap_or_default();
        // Exhaustive over the same set the catalog mounts from, so a
        // tool offered without an answer here does not compile.
        let outcome = match RunTool::parse(request.name.as_ref()) {
            Some(RunTool::CheckArtifact) => self.check_artifact(&args).await,
            Some(RunTool::PostFinding) => self.post_finding(args).await,
            Some(RunTool::UpdateFinding) => self.update_finding(args).await,
            Some(RunTool::WithdrawFinding) => self.withdraw_finding(args).await,
            Some(RunTool::GetBlackboard) => self.get_blackboard().await,
            Some(RunTool::TaskStatus) => self.task_status().await,
            Some(RunTool::Task) => self.task().await,
            Some(RunTool::CheckTask) => self.check_task().await,
            Some(RunTool::RequestScopeExpansion) => self.request_scope_expansion(args).await,
            Some(RunTool::Submit(kind)) => self.submit(kind, args).await,
            None => Err(RunToolError::UnknownTool {
                name: request.name.to_string(),
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
