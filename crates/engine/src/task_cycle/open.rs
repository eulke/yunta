//! Opening one attempt's session: the run-tools listener it is given, the
//! brief it starts from, and the dispatch that drives it to a terminal outcome.

use std::path::PathBuf;
use std::sync::Arc;

use yunta_core::port::SessionRequest;

use super::attempt::AttemptParams;
use super::dispatch::{dispatch_session, DispatchError};
use super::judge::scope_in_force;
use super::TaskCycleError;
use crate::run::session_plan::OpenedSession;
use crate::run_tools::{unrecorded_storage_cause, RunToolsSession, TaskAccess};

/// Opens a fresh session for one attempt and drives it to a terminal
/// outcome: a per-attempt run-tools listener (its bind failure degrades to
/// no tools, recorded, never fatal), the task's brief, and the dispatch
/// itself. Returns what the adapter staged, the dispatch outcome, the
/// tokens it spent, and the task access its run tools answered from.
pub(super) async fn open_and_dispatch(
    params: &AttemptParams<'_>,
    attempt: u32,
) -> Result<(Vec<PathBuf>, super::Dispatched, Arc<TaskAccess>), TaskCycleError> {
    let access = task_access(params, attempt);
    let OpenedSession { request, run_tools } = open_the_session(params, &access).await?;
    let last_staged = params.adapter.staged_paths(&request);
    // A session picked back up is told the answer to what it asked, not
    // the brief again; the brief is what a fresh session gets when the
    // adapter cannot pick the conversation up.
    let brief = request.prompt.clone();
    let request = match params.resume {
        Some(continuing) => crate::run::session_plan::with_prompt(
            request,
            crate::run_tools::continuation_notice(
                run_tools.as_ref(),
                &continuing.answer,
                crate::run_tools::Asker::Task,
            ),
        ),
        None => request,
    };
    let dispatched = dispatch(params, request, &brief, run_tools.as_ref()).await?;
    // A refusal the engine answered and could not write fails the attempt
    // with the storage cause the log gave it.
    let unrecorded = run_tools
        .as_ref()
        .and_then(|session| unrecorded_storage_cause(session.rails()));
    if let Some(source) = unrecorded {
        return Err(TaskCycleError::Audit {
            task: params.task.id.clone(),
            source,
        });
    }
    super::record_unlisted_tools(
        params.audit,
        params.adapter.id(),
        run_tools.as_ref(),
        &dispatched,
    )
    .await
    .map_err(|source| TaskCycleError::Audit {
        task: params.task.id.clone(),
        source,
    })?;
    Ok((last_staged, dispatched, access))
}

/// The task access one attempt's session answers its tools from: the task the
/// cycle holds, the scope it is held to (declared plus everything granted
/// before this attempt), and the unit it works in. A check stages its diff
/// through an index of its own, never the close's. The declaration starts
/// empty: a session declares at most one criterion.
fn task_access(params: &AttemptParams<'_>, attempt: u32) -> Arc<TaskAccess> {
    Arc::new(TaskAccess {
        task: params.task.clone(),
        scope: scope_in_force(params.task, params.already_granted_paths),
        protected: Arc::new(params.protected.clone()),
        unit: params.unit.clone(),
        index: crate::run_dir::index_for(&params.setup.run_dir, &params.unit.who)
            .with_extension("check"),
        cancel: params.supervision.cancel.clone(),
        staged: Default::default(),
        checks: Default::default(),
        plan: params.setup.plan.clone(),
        held: params.setup.held.clone(),
        declared: Arc::new(tokio::sync::Mutex::new(None)),
        expansion: expansion_of(params, attempt),
    })
}

/// Opens the attempt's session through the one door every session has: the
/// per-attempt listener (mandatory for a task session, which reads its task
/// through it), the brief, and the request itself.
async fn open_the_session(
    params: &AttemptParams<'_>,
    access: &Arc<TaskAccess>,
) -> Result<OpenedSession, TaskCycleError> {
    crate::run::session_plan::open_session(
        params.setup,
        crate::run::session_plan::SessionPlan {
            node: params.node,
            task: Some(access.clone()),
            prompt: crate::run_tools::words::task_brief(params.instruction, params.task),
            cwd: params.unit.worktree.as_path().to_path_buf(),
            profile: params.profile,
            budget: params.budget,
        },
        params.adapter,
        params.audit,
    )
    .await
    .map_err(|error| match error {
        crate::run::session_plan::OpenSessionError::Audit(source) => TaskCycleError::Audit {
            task: params.task.id.clone(),
            source,
        },
        crate::run::session_plan::OpenSessionError::RunTools(source) => TaskCycleError::RunTools {
            task: params.task.id.clone(),
            source,
        },
        crate::run::session_plan::OpenSessionError::Confinement(source) => {
            TaskCycleError::Confinement {
                task: params.task.id.clone(),
                source,
            }
        }
    })
}

/// Drives the opened session to its terminal outcome. A session picked back
/// up is resumed; a fresh one is handed `brief`, which is also what it is
/// told when the adapter cannot resume the conversation.
async fn dispatch(
    params: &AttemptParams<'_>,
    request: SessionRequest,
    brief: &str,
    run_tools: Option<&RunToolsSession>,
) -> Result<super::Dispatched, TaskCycleError> {
    let opening = crate::task_cycle::Opening {
        task: Some(&params.task.id),
        arrivals: run_tools.map(RunToolsSession::arrivals),
        rails: run_tools.map(RunToolsSession::rails),
        resume: params.resume.map(|continuing| crate::task_cycle::Resume {
            session: &continuing.session,
            fresh_prompt: Some(brief),
        }),
    };
    dispatch_session(
        params.adapter,
        request,
        params.cancel,
        params.audit,
        opening,
    )
    .await
    .map_err(|error| match error {
        DispatchError::Adapter(source) | DispatchError::Release(source) => TaskCycleError::Spawn {
            task: params.task.id.clone(),
            source,
        },
        DispatchError::Audit(source) => TaskCycleError::Audit {
            task: params.task.id.clone(),
            source,
        },
        DispatchError::NotStopped {
            detail,
            release,
            tokens,
        } => TaskCycleError::NotStopped {
            task: params.task.id.clone(),
            detail,
            release,
            tokens,
        },
    })
}

/// What a session's scope decision reads from its node and run: the node's
/// mode and grant cap, the run's file bound, what the batch's other tasks may
/// write, and the attempt the decision is made in.
fn expansion_of(params: &AttemptParams<'_>, attempt: u32) -> crate::run_tools::TaskExpansion {
    crate::run_tools::TaskExpansion {
        mode: params
            .scope_expansion
            .map(|policy| policy.mode)
            .unwrap_or_default(),
        max_files: params.max_expansion_files,
        max_per_run: params.scope_expansion.and_then(|policy| policy.max_per_run),
        beside: params.beside.to_vec(),
        attempt,
    }
}
