//! `kind: prompt` — one agent session: its prompt, its runner, the
//! per-run tools it may mount, and the orphaned session it may resume.

use std::path::PathBuf;

use tokio_util::sync::CancellationToken;
use yunta_core::events::EventPayload;
use yunta_core::events::Failure;
use yunta_core::events::NodeOutcome;
use yunta_core::events::OrphanedSession;
use yunta_core::events::{AccountLimit, FailureCause, LimitSource, SessionRefusal};
use yunta_core::port::{AgentError, SessionRequest};
use yunta_core::{AdapterId, Node, PromptSource};

use crate::run_dir::Opening;
use crate::run_tools::RunToolsSession;
use crate::task_cycle::{
    dispatch_session, not_stopped_failure, DispatchError, DispatchOutcome, Dispatched, SessionSetup,
};

use super::node_close::{
    close_node, fail_unstopped, fail_with_staged, fail_with_tokens_staged, Close,
};
use super::node_exec::{cancelled_end, open_staging, render_or_fail, session_profile, NodeEnd};
use super::runner_resolve::{report_declarative_network, resolve_node_runner};
use super::session_plan::{OpenSessionError, OpenedSession};
use super::step::Step;
use super::{RunCtx, RunError};
use yunta_core::events::SessionEvent;

/// The node's prompt text: frozen file content from the manifest when the
/// workflow declared `{file: ...}`, the inline string otherwise — never a
/// re-read from disk.
pub(super) fn prompt_text<'a>(
    ctx: &'a RunCtx<'_>,
    node: &'a Node,
    prompt: &'a PromptSource,
) -> &'a str {
    match prompt {
        PromptSource::Inline(text) => text,
        PromptSource::File(_) => ctx
            .manifest
            .prompts
            .get(&node.id)
            .map(String::as_str)
            // A file prompt missing from the manifest cannot happen for a
            // manifest built by `build_manifest` (it reads every file
            // prompt or errors); an empty prompt for a hand-edited
            // manifest fails the session visibly downstream.
            .unwrap_or(""),
    }
}

/// The whole text one session receives: the node's context ahead of the
/// author's prompt.
async fn assemble_prompt(
    ctx: &RunCtx<'_>,
    node: &Node,
    prompt: &PromptSource,
    naming: yunta_core::ToolNaming,
    cancel: &CancellationToken,
) -> Result<Step<String>, RunError> {
    let rendered = match render_or_fail(ctx, node, prompt_text(ctx, node, prompt)).await? {
        Step::Value(rendered) => rendered,
        Step::Ended(end) => return Ok(Step::Ended(end)),
    };
    let context_block =
        match super::context_resolve::resolve_and_assemble(ctx, node, naming, cancel).await? {
            Step::Value(block) => block,
            Step::Ended(end) => return Ok(Step::Ended(end)),
        };
    Ok(Step::Value(match context_block {
        Some(block) => format!("{block}\n{rendered}"),
        None => rendered,
    }))
}

/// The session an orphaned node picks back up under
/// `on_interrupt: resume_session`, if any.
///
/// Anything less than a clean resume — no capability, no recorded
/// session — degrades to a fresh session WITH an event, never silently.
async fn resume_target(
    ctx: &RunCtx<'_>,
    node: &Node,
    adapter: &dyn yunta_core::port::Adapter,
) -> Result<Option<yunta_core::SessionId>, RunError> {
    let policy = node
        .on_interrupt
        .unwrap_or(ctx.manifest.config.resolved_on_interrupt());
    if policy != yunta_core::OnInterrupt::ResumeSession {
        return Ok(None);
    }
    let state = ctx.run_view().await?.state;
    // A session that worked in the run's own tree started in the checkout
    // the run left behind when it woke somewhere else.
    let left_behind = !crate::audits(node) && state.run.moved_on_waking();
    let orphaned = state
        .nodes
        .get(&node.id)
        .and_then(|record| record.orphaned_session.clone());
    match orphaned {
        Some(OrphanedSession::Open(session_id)) if !left_behind => {
            match crate::run::capability::require(
                ctx,
                adapter,
                yunta_core::Capability::ResumeSession,
                node,
            )
            .await?
            {
                crate::run::capability::Decision::Granted => return Ok(Some(session_id)),
                crate::run::capability::Decision::Degraded => {}
                crate::run::capability::Decision::Refused(error) => return Err(error),
            }
        }
        // The capability is not what is missing here — there is nothing
        // to resume, or nothing to resume from where the run now works.
        // The run still says so, with the same fallback it took, so a
        // reader knows the interrupted node started over.
        Some(_) => starts_over(ctx, node, adapter).await?,
        None => {}
    }
    Ok(None)
}

/// Says an interrupted node starts a fresh session instead of the one it
/// left.
async fn starts_over(
    ctx: &RunCtx<'_>,
    node: &Node,
    adapter: &dyn yunta_core::port::Adapter,
) -> Result<(), RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::Session(SessionEvent::CapabilityDegraded(
            yunta_core::events::CapabilityDegradedPayload::new(
                yunta_core::Capability::ResumeSession,
                adapter.id().clone(),
                yunta_core::port::Policy::FreshSession,
            ),
        )),
    )
    .await?;
    Ok(())
}

/// One `kind: prompt` node: its context, its prompt, one session, and
/// the close that verifies what it declared.
pub(super) async fn execute_prompt(
    ctx: &RunCtx<'_>,
    node: &Node,
    prompt: &PromptSource,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    // The runner first: what the session is shown names the run's tools
    // the way its CLI does, and only the runner says which CLI that is.
    let chosen = match resolve_node_runner(ctx, node).await? {
        Step::Value(chosen) => chosen,
        Step::Ended(end) => return Ok(end),
    };
    let adapter = &ctx.adapters[&chosen.adapter];
    let naming = adapter.capabilities().tool_naming;
    let rendered = match assemble_prompt(ctx, node, prompt, naming, cancel).await? {
        Step::Value(rendered) => rendered,
        Step::Ended(end) => return Ok(end),
    };

    report_declarative_network(ctx, node, adapter.as_ref()).await?;
    let setup = match super::session_plan::resolve_setup(ctx, node, &chosen).await? {
        Ok(setup) => setup,
        Err(end) => return Ok(end),
    };
    let opened = match open_node_session(ctx, node, &setup, adapter.as_ref(), rendered).await? {
        Ok(opened) => opened,
        Err(end) => return Ok(end),
    };
    let (dispatched, staged) =
        match dispatch_node_session(ctx, node, adapter.as_ref(), opened, cancel).await? {
            Step::Value(session) => session,
            Step::Ended(end) => return Ok(end),
        };
    close_on_outcome(ctx, node, adapter.as_ref(), dispatched, &staged).await
}

/// The node's one session, opened on its adapter. A session the engine cannot
/// confine, and a node that cannot proceed without the run tools it declared,
/// are both refused here, before a token is spent, and the node ends on the
/// reason.
async fn open_node_session(
    ctx: &RunCtx<'_>,
    node: &Node,
    setup: &SessionSetup,
    adapter: &dyn yunta_core::port::Adapter,
    rendered: String,
) -> Result<Result<OpenedSession, NodeEnd>, RunError> {
    let opened = super::session_plan::open_session(
        setup,
        super::session_plan::SessionPlan {
            node,
            task: None,
            prompt: rendered,
            cwd: ctx.worktree.to_path_buf(),
            profile: session_profile(node),
            budget: ctx.session_budget().await?,
        },
        adapter,
        Some((ctx as &dyn crate::task_cycle::SessionObserver, &node.id)),
    )
    .await;
    match opened {
        Ok(opened) => Ok(Ok(opened)),
        // A node that cannot proceed without the run tools it declared:
        // refused before a token is spent, naming what has no way in.
        Err(OpenSessionError::RunTools(error)) => {
            Ok(Err(
                super::node_close::fail(ctx, node, error.to_string()).await?
            ))
        }
        // A session the engine cannot confine is refused the same way, naming
        // what the confinement could not hold.
        Err(OpenSessionError::Confinement(error)) => {
            Ok(Err(
                super::node_close::fail(ctx, node, error.to_string()).await?
            ))
        }
        Err(OpenSessionError::Audit(source)) => Err(RunError::Storage(source)),
    }
}

/// The one session of an attempt, dispatched: how it resumes, what it is told,
/// the staging it writes into, and the drive to its outcome. Returns what the
/// session ended on and the files its adapter staged for itself, or the node's
/// end when the session could not be stopped.
async fn dispatch_node_session(
    ctx: &RunCtx<'_>,
    node: &Node,
    adapter: &dyn yunta_core::port::Adapter,
    opened: OpenedSession,
    cancel: &CancellationToken,
) -> Result<Step<(Dispatched, Vec<PathBuf>)>, RunError> {
    let OpenedSession { request, run_tools } = opened;
    let continuing = ctx.unit.and_then(|mine| mine.continues);
    let resume_session = resume_and_stage(ctx, node, adapter, continuing).await?;
    let staged = adapter.staged_paths(&request);
    let (request, brief) =
        attempt_request(ctx, node, request, run_tools.as_ref(), continuing).await?;
    let dispatched = match dispatch_session(
        adapter,
        request,
        cancel,
        Some((ctx as &dyn crate::task_cycle::SessionObserver, &node.id)),
        attempt_opening(
            run_tools.as_ref(),
            resume_session.as_ref(),
            continuing,
            &brief,
        ),
    )
    .await
    {
        Ok(dispatched) => dispatched,
        Err(error) => return Ok(Step::Ended(dispatch_failed(ctx, node, error).await?)),
    };
    crate::task_cycle::record_unlisted_tools(
        Some((ctx as &dyn crate::task_cycle::SessionObserver, &node.id)),
        adapter.id(),
        run_tools.as_ref(),
        &dispatched,
    )
    .await
    .map_err(RunError::Storage)?;
    Ok(Step::Value((dispatched, staged)))
}

/// The end of a dispatch that failed. A session the engine could not stop ends
/// the node with that fact and with what the session spent, not with a run
/// error; every other failure is the run's.
async fn dispatch_failed(
    ctx: &RunCtx<'_>,
    node: &Node,
    error: DispatchError,
) -> Result<NodeEnd, RunError> {
    match error {
        DispatchError::NotStopped {
            detail,
            release,
            tokens,
        } => {
            let failure = not_stopped_failure(&detail, release.as_deref());
            fail_unstopped(ctx, node, failure, tokens).await
        }
        DispatchError::Adapter(source) | DispatchError::Release(source) => Err(RunError::Spawn {
            node: node.id.clone(),
            source,
        }),
        DispatchError::Audit(source) => Err(RunError::Storage(source)),
    }
}

/// The session an attempt picks back up, if it picks one up, and the staging
/// it writes into. A session continuing here keeps what it wrote, and any
/// other session opens on nothing, so no earlier attempt's file closes this
/// one. Only here is the answer known: it takes the node's policy, the log,
/// and the runner this node just resolved.
async fn resume_and_stage(
    ctx: &RunCtx<'_>,
    node: &Node,
    adapter: &dyn yunta_core::port::Adapter,
    continuing: Option<&crate::task_cycle::Continuing>,
) -> Result<Option<yunta_core::SessionId>, RunError> {
    // An attempt picking up the session a person answered resumes it, told
    // the answer; an interrupted one resumes under its policy.
    let resume_session = match continuing {
        Some(continuing) => Some(continuing.session.clone()),
        None => resume_target(ctx, node, adapter).await?,
    };
    open_staging(
        ctx,
        node,
        match resume_session {
            Some(_) => Opening::ContinuedSession,
            None => Opening::Fresh,
        },
    )
    .await?;
    Ok(resume_session)
}

/// The request one attempt hands over, and the brief it is told. A fresh
/// session a person sent back here is told why, and a session picked back up
/// is told the answer to what it asked.
async fn attempt_request(
    ctx: &RunCtx<'_>,
    node: &Node,
    request: SessionRequest,
    run_tools: Option<&RunToolsSession>,
    continuing: Option<&crate::task_cycle::Continuing>,
) -> Result<(SessionRequest, String), RunError> {
    // A fresh session a person sent back here is told why — their review, or
    // the tests it wrote that they accepted are wrong — and where what the
    // node handed over is: a session picked back up has both already, and is
    // told only the answer.
    let sent_back = super::continuation::sent_back(ctx, &node.id).await?;
    let brief = match &sent_back {
        Some(notice) => format!("{}{notice}", request.prompt),
        None => request.prompt.clone(),
    };
    let request = match continuing {
        Some(continuing) => super::session_plan::with_prompt(
            request,
            crate::run_tools::continuation_notice(
                run_tools,
                &continuing.answer,
                crate::run_tools::Asker::Node,
            ),
        ),
        None if sent_back.is_some() => super::session_plan::with_prompt(request, brief.clone()),
        None => request,
    };
    Ok((request, brief))
}

/// How the attempt's session opens: the run tools whose calls say what reached
/// the server, and the conversation it picks back up, with the brief a fresh
/// session would be given.
fn attempt_opening<'a>(
    run_tools: Option<&'a RunToolsSession>,
    resume: Option<&'a yunta_core::SessionId>,
    continuing: Option<&crate::task_cycle::Continuing>,
    brief: &'a str,
) -> crate::task_cycle::Opening<'a> {
    crate::task_cycle::Opening {
        task: None,
        arrivals: run_tools.map(RunToolsSession::arrivals),
        // A continued session has the brief to fall back on; an
        // interrupted one was already checked to be resumable.
        resume: resume.map(|session| crate::task_cycle::Resume {
            session,
            fresh_prompt: continuing.map(|_| brief),
        }),
    }
}

/// How an attempt's one session ended, closed into the node: the summary a
/// completed session leaves, or the failure, the budget stop, or the
/// cancellation that ended it otherwise.
async fn close_on_outcome(
    ctx: &RunCtx<'_>,
    node: &Node,
    adapter: &dyn yunta_core::port::Adapter,
    dispatched: Dispatched,
    staged: &[PathBuf],
) -> Result<NodeEnd, RunError> {
    let Dispatched {
        outcome, tokens, ..
    } = dispatched;
    match outcome {
        DispatchOutcome::Completed { summary } => {
            let node_outcome = NodeOutcome::Summary(summary);
            close_node(ctx, node, Close::new(node_outcome, tokens).staged(staged)).await
        }
        DispatchOutcome::Failed { error, cause } => {
            let failure = session_failure(adapter.id(), error, cause);
            fail_with_staged(ctx, node, failure, tokens, staged).await
        }
        // No terminal event means the engine records the death — the
        // adapter never invents a terminal of its own — with how the
        // process went, which is what a reader needs to tell a CLI that
        // refused its configuration from one that merely stopped.
        DispatchOutcome::Crashed { exit } => {
            let death = Failure::session_died(adapter.id().clone(), exit);
            fail_with_staged(ctx, node, death, tokens, staged).await
        }
        DispatchOutcome::BudgetExceeded { reason } => {
            fail_with_tokens_staged(ctx, node, reason, tokens, staged).await
        }
        DispatchOutcome::Cancelled => cancelled_end(ctx, node).await,
    }
}

/// The failure a session that failed on its own leaves on its node: the
/// account's limit the adapter named, or the refusal it gave, each with
/// the adapter's own message.
fn session_failure(adapter: &AdapterId, error: AgentError, cause: FailureCause) -> Failure {
    match cause {
        FailureCause::Limit { resets_at } => Failure::Limit {
            limit: AccountLimit {
                source: LimitSource::Session {
                    adapter: adapter.clone(),
                },
                resets_at,
                message: error.message,
            },
        },
        FailureCause::Refused(cause) => Failure::SessionRefused {
            session_refused: SessionRefusal {
                adapter: adapter.clone(),
                cause,
                message: error.message,
            },
        },
    }
}
