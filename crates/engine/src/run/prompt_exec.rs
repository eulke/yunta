//! `kind: prompt` — one agent session: its prompt, its runner, the
//! per-run tools it may mount, and the orphaned session it may resume.

use tokio_util::sync::CancellationToken;
use yunta_core::events::EventPayload;
use yunta_core::events::OrphanedSession;
use yunta_core::{Node, PromptSource};

use crate::run_dir::Opening;
use crate::task_cycle::{dispatch_session, DispatchOutcome};

use super::node_close::{close_node, fail_with_tokens, Close};
use super::node_exec::{cancelled_end, open_staging, render_or_fail, session_profile, NodeEnd};
use super::runner_resolve::{report_declarative_network, resolve_node_runner};
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
    cancel: &CancellationToken,
) -> Result<Step<String>, RunError> {
    let rendered = match render_or_fail(ctx, node, prompt_text(ctx, node, prompt)).await? {
        Step::Value(rendered) => rendered,
        Step::Ended(end) => return Ok(Step::Ended(end)),
    };
    let context_block =
        match super::context_resolve::resolve_and_assemble(ctx, node, cancel).await? {
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
    adapter_id: &yunta_core::AdapterId,
) -> Result<Option<yunta_core::SessionId>, RunError> {
    let policy = node
        .on_interrupt
        .unwrap_or(ctx.manifest.config.resolved_on_interrupt());
    if policy != yunta_core::OnInterrupt::ResumeSession {
        return Ok(None);
    }
    let degraded = || {
        EventPayload::Session(SessionEvent::CapabilityDegraded(
            yunta_core::events::CapabilityDegradedPayload::new(
                yunta_core::Capability::ResumeSession,
                adapter_id.clone(),
                yunta_core::events::Policy::FreshSession,
            ),
        ))
    };
    match crate::replay::derive(&ctx.load_events().await?)
        .nodes
        .get(&node.id)
        .and_then(|record| record.orphaned_session.clone())
    {
        Some(OrphanedSession::Open(session_id)) => {
            if adapter
                .capabilities()
                .declares(yunta_core::Capability::ResumeSession)
            {
                return Ok(Some(session_id));
            }
            ctx.emit(Some(&node.id), degraded()).await?;
        }
        Some(OrphanedSession::NoneRecorded) => {
            ctx.emit(Some(&node.id), degraded()).await?;
        }
        None => {}
    }
    Ok(None)
}

/// One `kind: prompt` node: its context, its prompt, one session, and
/// the close that verifies what it declared.
pub(super) async fn execute_prompt(
    ctx: &RunCtx<'_>,
    node: &Node,
    prompt: &PromptSource,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let rendered = match assemble_prompt(ctx, node, prompt, cancel).await? {
        Step::Value(rendered) => rendered,
        Step::Ended(end) => return Ok(end),
    };
    let chosen = match resolve_node_runner(ctx, node).await? {
        Step::Value(chosen) => chosen,
        Step::Ended(end) => return Ok(end),
    };

    let adapter = &ctx.adapters[&chosen.adapter];
    report_declarative_network(ctx, node, adapter.as_ref(), &chosen.adapter).await?;
    let setup = match super::session_plan::resolve_setup(ctx, node, &chosen).await? {
        Ok(setup) => setup,
        Err(end) => return Ok(end),
    };
    let super::session_plan::OpenedSession {
        request,
        run_tools: _run_tools,
    } = match super::session_plan::open_session(
        &setup,
        super::session_plan::SessionPlan {
            node,
            task: None,
            prompt: rendered,
            cwd: ctx.worktree.to_path_buf(),
            profile: session_profile(node),
            budget: ctx.session_budget().await?,
        },
        adapter.as_ref(),
        Some((ctx as &dyn crate::task_cycle::SessionObserver, &node.id)),
    )
    .await
    {
        Ok(opened) => opened,
        // A node that cannot proceed without the run tools it declared:
        // refused before a token is spent, naming what has no way in.
        Err(super::session_plan::OpenSessionError::RunTools(error)) => {
            return super::node_close::fail(ctx, node, error.to_string(), false).await
        }
        Err(super::session_plan::OpenSessionError::Audit(source)) => {
            return Err(RunError::Storage(source))
        }
    };

    let resume_session = resume_target(ctx, node, adapter.as_ref(), &chosen.adapter).await?;
    // The staging is the session's. A session continuing here already
    // wrote in it and what it left is work it did; a fresh session —
    // including one replacing an interrupted session the adapter cannot
    // resume — opens on nothing, so no earlier attempt's file closes
    // this one. Only here is the answer known: it takes the node's
    // policy, the log, AND the runner this node just resolved.
    open_staging(
        ctx,
        node,
        match resume_session {
            Some(_) => Opening::ContinuedSession,
            None => Opening::Fresh,
        },
    )
    .await?;

    let staged = adapter.staged_paths(&request);
    let (outcome, tokens) = dispatch_session(
        adapter.as_ref(),
        request,
        cancel,
        Some((ctx as &dyn crate::task_cycle::SessionObserver, &node.id)),
        resume_session.as_ref(),
    )
    .await
    .map_err(|error| match error {
        crate::task_cycle::DispatchError::Adapter(source) => RunError::Spawn {
            node: node.id.clone(),
            source,
        },
        crate::task_cycle::DispatchError::Audit(source) => RunError::Storage(source),
    })?;

    match outcome {
        DispatchOutcome::Completed { summary } => {
            close_node(ctx, node, Close::new(summary, tokens).staged(&staged)).await
        }
        DispatchOutcome::Failed { message, retryable } => {
            fail_with_tokens(ctx, node, message, retryable, tokens).await
        }
        // No terminal event means the engine synthesizes a retryable
        // failure — the adapter never invents one.
        DispatchOutcome::Crashed => {
            fail_with_tokens(
                ctx,
                node,
                "session ended without a terminal event".to_string(),
                true,
                tokens,
            )
            .await
        }
        DispatchOutcome::BudgetExceeded { reason } => {
            fail_with_tokens(ctx, node, reason, false, tokens).await
        }
        DispatchOutcome::Cancelled => cancelled_end(ctx, node).await,
    }
}
