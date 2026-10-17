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

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use super::catalog::RunTool;
pub(crate) use super::error::RunToolError;
use super::words;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ListToolsResult, PaginatedRequestParams,
};
use rmcp::model::{ServerCapabilities, ServerConfig};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use tokio_util::sync::CancellationToken;
use yunta_core::events::{EventPayload, StoredEvent};
use yunta_core::{ArtifactSpec, NodeId, NodeKind};

use super::host::{NodeScopeAccess, RunToolsAccess, RunToolsHost, TaskAccess};
use crate::run_log::RunLog;

/// The run tools of one session. Which of them are even *listed* depends
/// on the session: `yunta_get_blackboard` only inside a
/// `coordination: blackboard` group (for `independent` they aren't
/// mounted at all), the task tools only for task sessions,
/// `yunta_check_scope` only for a node's own session with a scope of its
/// own, `yunta_request_scope_expansion` for either when an answer can
/// widen it, and one submission tool per kind of document this node
/// declares.
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
    /// The node's own scope, for a node's session when the node declares
    /// one: what `yunta_check_scope` judges against, and whether the
    /// session may ask for more.
    pub(super) node_scope: Option<Arc<NodeScopeAccess>>,
    pub(super) cwd: PathBuf,
    /// What stops a command a tool runs for this session: the task's own
    /// token, and the session's end.
    pub(super) stop: CancellationToken,
    /// The artifacts this node's close will verify, names already
    /// rendered.
    pub(super) declared: Vec<ArtifactSpec>,
    /// How the session's CLI names these tools to its model.
    pub(super) naming: yunta_core::ToolNaming,
    /// What the session's hooks are judged against, and the refusal the log
    /// could not keep, if one was.
    pub(super) rails: Arc<super::rail::SessionRails>,
    /// Whether the session's CLI ever listed these tools or called one:
    /// what says they reached it when its own stream does not.
    pub(super) listed: Arc<std::sync::atomic::AtomicBool>,
    /// The calls this server was reached by, and which of them have not
    /// finished with success.
    pub(super) arrivals: Arc<Arrivals>,
}

impl SessionTools {
    /// The tools one session is served: what its node's setup allows,
    /// what its work is held to — a loop's task, or the node's own scope
    /// — where it works, and what stops what a tool runs for it.
    pub(super) fn new(
        access: RunToolsAccess,
        (task, node_scope): (Option<Arc<TaskAccess>>, Option<Arc<NodeScopeAccess>>),
        cwd: PathBuf,
        stop: CancellationToken,
        rails: Arc<super::rail::SessionRails>,
        listed: Arc<std::sync::atomic::AtomicBool>,
        arrivals: Arc<Arrivals>,
    ) -> Self {
        let RunToolsAccess {
            host,
            node,
            node_kind,
            declared,
            naming,
        } = access;
        SessionTools {
            host,
            node,
            node_kind,
            task,
            node_scope,
            cwd,
            stop,
            declared,
            naming,
            rails,
            listed,
            arrivals,
        }
    }

    /// What this session's model calls `tool` by.
    pub(super) fn called(&self, tool: RunTool) -> String {
        tool.called(self.naming)
    }
}

impl SessionTools {
    /// The run's events, as every tool that derives state reads them.
    pub(super) async fn events(&self) -> Result<Vec<StoredEvent>, RunToolError> {
        self.host
            .storage
            .events_for_run(self.host.run_id.clone())
            .await
            .map_err(RunToolError::from)
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

    /// Records that this binary refused a call to `tool`, and why: the log
    /// says what the session was answered, in a word a reader can count.
    async fn refused(&self, tool: RunTool, error: &RunToolError) {
        let Some(reason) = error.refusal() else {
            return;
        };
        let payload = EventPayload::Session(yunta_core::events::SessionEvent::RunToolRefused(
            yunta_core::events::RunToolRefusedPayload { tool, reason },
        ));
        if let Err(error) = self.append(payload).await {
            self.rails.left_unrecorded(error);
        }
    }

    /// Records `payload` against this session's run and node, stamped with
    /// the run's own clock, and leaves a failure to the caller to answer.
    pub(super) async fn append(&self, payload: EventPayload) -> Result<(), RunToolError> {
        self.log()
            .record(Some(&self.node), payload)
            .await
            .map(|_| ())
            .map_err(RunToolError::from)
    }

    /// Records `runs` as the log keeps them, what each printed kept with
    /// the run's objects; `task` names whose check could not keep it.
    pub(super) async fn recorded(
        &self,
        task: &yunta_core::TaskId,
        runs: &[crate::task_cycle::CriterionRun],
    ) -> Result<Vec<yunta_core::events::CriterionResult>, RunToolError> {
        let objects = crate::artifacts::store::ObjectStore::at(&self.host.run_dir)
            .redacting(&self.host.redactor);
        crate::task_cycle::recorded(runs, &objects)
            .await
            .map_err(|source| RunToolError::Check {
                source: crate::task_cycle::TaskCycleError::KeepOutput {
                    task: task.clone(),
                    source,
                },
            })
    }
}

impl ServerHandler for SessionTools {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        self.listed
            .store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(crate::mcp::tool_list(super::catalog::mounted(self)))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, McpError> {
        self.listed
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let args = request.arguments.unwrap_or_default();
        // A tool this server never offered is a call the protocol refuses,
        // not a tool that ran and failed.
        let Some(tool) = RunTool::parse(request.name.as_ref()) else {
            return Err(McpError::invalid_params(
                words::unknown_tool(&request.name),
                None,
            ));
        };
        self.arrivals.reached(tool);
        // Exhaustive over the same set the catalog mounts from, so a
        // tool offered without an answer here does not compile.
        let outcome = match tool {
            RunTool::CheckArtifact => self.check_artifact(&args).await,
            RunTool::PostFinding => self.post_finding(args).await,
            RunTool::UpdateFinding => self.update_finding(args).await,
            RunTool::WithdrawFinding => self.withdraw_finding(args).await,
            RunTool::Findings => self.findings_standing().await,
            RunTool::AnswerFinding => self.answer_finding(args).await,
            RunTool::GetBlackboard => self.get_blackboard().await,
            RunTool::TaskStatus => self.task_status().await,
            RunTool::Task => self.task().await,
            RunTool::CheckTask => self.check_task().await,
            RunTool::CheckScope => self.check_scope().await,
            RunTool::RequestScopeExpansion => self.request_scope_expansion(args).await,
            RunTool::DeclareDeviation => self.declare_deviation(args).await,
            RunTool::Submit(kind) => self.submit(kind, args).await,
        };
        Ok(match outcome {
            Ok(text) => {
                self.arrivals.answered(tool);
                CallToolResult::success(vec![ContentBlock::text(text)]).into()
            }
            // A call the run stopped is not a refusal: the protocol reports it
            // as unanswered, and the log keeps no refusal for it.
            Err(stopped @ RunToolError::Cancelled) => {
                return Err(McpError::internal_error(words::tool_error(&stopped), None))
            }
            Err(error) => {
                self.refused(tool, &error).await;
                CallToolResult::error(vec![ContentBlock::text(words::tool_error(&error))]).into()
            }
        })
    }
}

/// Calls to each run tool that reached this server and have not finished with success.
///
/// Counted per tool, not per call: the CLI gives no call id the listener sees, so a
/// rejection reads as a failed call while another call to the same tool is open.
#[derive(Default)]
pub(crate) struct Arrivals(Mutex<BTreeMap<RunTool, u32>>);

impl Arrivals {
    /// Recorded before the server answers; a CLI reports a rejection only after that.
    pub(crate) fn reached(&self, tool: RunTool) {
        *self.open().entry(tool).or_default() += 1;
    }

    /// The call finished with success, so it is no longer open.
    pub(crate) fn answered(&self, tool: RunTool) {
        if let Some(open) = self.open().get_mut(&tool) {
            *open = open.saturating_sub(1);
        }
    }

    /// Whether a rejection of `tool` reached the server; consumes one open call.
    pub(crate) fn take_arrival(&self, tool: RunTool) -> bool {
        match self.open().get_mut(&tool) {
            Some(open) if *open > 0 => {
                *open -= 1;
                true
            }
            _ => false,
        }
    }

    /// The open calls, held for one change: a poisoned lock still holds the counts.
    fn open(&self) -> std::sync::MutexGuard<'_, BTreeMap<RunTool, u32>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rejection_is_failed_only_for_a_call_that_reached_the_server() {
        let arrivals = Arrivals::default();
        arrivals.reached(RunTool::Findings);
        arrivals.answered(RunTool::Findings);
        assert!(
            !arrivals.take_arrival(RunTool::Findings),
            "a blocked call after a success"
        );
        arrivals.reached(RunTool::Findings); // refused, in flight
        arrivals.reached(RunTool::Findings); // reached the server as well
        assert!(arrivals.take_arrival(RunTool::Findings));
        assert!(arrivals.take_arrival(RunTool::Findings));
        assert!(
            !arrivals.take_arrival(RunTool::Findings),
            "no call left to claim"
        );
    }

    #[test]
    fn interleaved_calls_claim_rejections_per_tool() {
        let arrivals = Arrivals::default();
        arrivals.reached(RunTool::Findings);
        arrivals.reached(RunTool::Task);
        arrivals.answered(RunTool::Findings);
        assert!(
            !arrivals.take_arrival(RunTool::Findings),
            "its call succeeded"
        );
        assert!(
            arrivals.take_arrival(RunTool::Task),
            "its call is still open"
        );
        assert!(
            !arrivals.take_arrival(RunTool::Task),
            "no call left to claim"
        );
    }
}
