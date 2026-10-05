//! `kind: prompt` — one agent session: its prompt, its runner, the
//! per-run tools it may mount, and the orphaned session it may resume.

use tokio_util::sync::CancellationToken;
use yunta_core::events::EventPayload;
use yunta_core::events::Failure;
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
    let state = crate::replay::derive(&ctx.load_events().await?);
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
                yunta_core::events::Policy::FreshSession,
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
    let super::session_plan::OpenedSession { request, run_tools } =
        match super::session_plan::open_session(
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

    // An attempt picking up the session a person answered resumes it,
    // told the answer; an interrupted one resumes under its policy.
    let continuing = ctx.unit.and_then(|mine| mine.continues);
    let resume_session = match continuing {
        Some(continuing) => Some(continuing.session.clone()),
        None => resume_target(ctx, node, adapter.as_ref()).await?,
    };
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
    // A fresh session a person sent back here is told why — their
    // review, or the tests it wrote that they accepted are wrong — and
    // where what the node handed over is: a session picked back up has
    // both already, and is told only the answer.
    let sent_back = super::continuation::sent_back(ctx, &node.id).await?;
    let brief = match &sent_back {
        Some(notice) => format!("{}{notice}", request.prompt),
        None => request.prompt.clone(),
    };
    let request = match continuing {
        Some(continuing) => super::session_plan::with_prompt(
            request,
            crate::run_tools::continuation_notice(
                run_tools.as_ref(),
                &continuing.answer,
                crate::run_tools::Asker::Node,
            ),
        ),
        None if sent_back.is_some() => super::session_plan::with_prompt(request, brief.clone()),
        None => request,
    };
    let crate::task_cycle::Dispatched {
        outcome, tokens, ..
    } = dispatch_session(
        adapter.as_ref(),
        request,
        cancel,
        Some((ctx as &dyn crate::task_cycle::SessionObserver, &node.id)),
        crate::task_cycle::Opening {
            task: None,
            // A continued session has the brief to fall back on; an
            // interrupted one was already checked to be resumable.
            resume: resume_session
                .as_ref()
                .map(|session| crate::task_cycle::Resume {
                    session,
                    fresh_prompt: continuing.map(|_| brief.as_str()),
                }),
        },
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
        // No terminal event means the engine records the death — the
        // adapter never invents a terminal of its own — with how the
        // process went, which is what a reader needs to tell a CLI that
        // refused its configuration from one that merely stopped.
        DispatchOutcome::Crashed { exit } => {
            super::node_close::fail_with(
                ctx,
                node,
                Failure::session_died(adapter.id().clone(), exit),
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
