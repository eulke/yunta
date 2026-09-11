//! `kind: prompt` — one agent session: its prompt, its runner, the
//! per-run tools it may mount, and the orphaned session it may resume.

use tokio_util::sync::CancellationToken;
use yunta_adapters::SessionRequest;
use yunta_core::events::EventPayload;
use yunta_core::{Node, PromptSource};

use crate::task_cycle::{dispatch_session, DispatchOutcome};

use super::node_close::{close_node_staged, fail, fail_with_tokens, Closed};
use super::node_exec::{cancelled_end, render_or_fail, session_profile, NodeEnd};
use super::runner_resolve::{open_run_tools, report_declarative_network, resolve_node_runner};
use super::step::Step;
use super::{RunCtx, RunError};

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

/// One `kind: prompt` node, including the repair cycle its declared
/// artifacts earn.
///
/// Verification is unchanged: an artifact that cannot be read still
/// fails the node. What changes is that a failure a rewrite could fix no
/// longer ends there. The session that wrote the file is gone by the
/// time the file is read, so the correction is a fresh session with the
/// same prompt plus the diagnostics — the same shape the task cycle
/// already has for work, applied to the frontier that had none.
///
/// The budget is `limits.max_artifact_repairs`, declared and frozen in
/// the manifest like every other limit that governs a run, so the
/// receipt can account for it.
pub(super) async fn execute_prompt(
    ctx: &RunCtx<'_>,
    node: &Node,
    prompt: &PromptSource,
    cancel: &CancellationToken,
) -> Result<NodeEnd, RunError> {
    let budget = ctx.manifest.config.resolved_max_artifact_repairs();
    let mut repair: Option<String> = None;
    for spent in 0..=budget {
        // The first attempt is the one `execute_node` already announced;
        // every repair is a fresh attempt on the log, which is what makes
        // it visible in `status`, countable in `stats`, and replayable —
        // `node_failed` followed by `node_started` is a sequence the
        // derivation already accepts.
        if spent > 0 {
            let attempt = started_count(ctx, node).await? + 1;
            ctx.emit(
                Some(&node.id),
                EventPayload::NodeStarted(yunta_core::events::NodeStartedPayload { attempt }),
            )
            .await?;
        }
        let closed = one_session(ctx, node, prompt, cancel, repair.as_deref()).await?;
        match closed.repair {
            Some(instruction) if spent < budget => repair = Some(instruction),
            _ => return Ok(closed.end),
        }
    }
    // `budget + 1` attempts all ended repairable: the loop above returns
    // on the last one, so this is unreachable by construction and the
    // node's own failure is already on the log either way.
    Ok(NodeEnd::Failed)
}

/// The whole text one session receives: the node's context, the
/// author's prompt, and — on a repair — what the previous attempt got
/// wrong.
///
/// The repair goes last, after everything stable. What the previous
/// attempt got wrong is the most volatile thing in the prompt, and
/// putting it at the end leaves the cacheable prefix untouched.
async fn assemble_prompt(
    ctx: &RunCtx<'_>,
    node: &Node,
    prompt: &PromptSource,
    cancel: &CancellationToken,
    repair: Option<&str>,
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
    let rendered = match context_block {
        Some(block) => format!("{block}\n{rendered}"),
        None => rendered,
    };
    Ok(Step::Value(match repair {
        Some(instruction) => format!("{rendered}\n\n{instruction}"),
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
    adapter: &dyn yunta_adapters::Adapter,
    adapter_id: &yunta_core::AdapterId,
) -> Result<Option<yunta_core::SessionId>, RunError> {
    let policy = node
        .on_interrupt
        .unwrap_or(ctx.manifest.config.resolved_on_interrupt());
    if policy != yunta_core::OnInterrupt::ResumeSession {
        return Ok(None);
    }
    let degraded = |policy_applied: &str| {
        EventPayload::CapabilityDegraded(yunta_core::events::CapabilityDegradedPayload {
            capability: yunta_core::Capability::ResumeSession,
            adapter: adapter_id.clone(),
            policy_applied: policy_applied.to_string(),
        })
    };
    match orphaned_session(&ctx.load_events().await?, &node.id) {
        OrphanedSession::Open(session_id) => {
            if adapter
                .capabilities()
                .declares(yunta_core::Capability::ResumeSession)
            {
                return Ok(Some(session_id));
            }
            ctx.emit(
                Some(&node.id),
                degraded(
                    "restart_node — the adapter declares no session resume; a fresh session \
                     replaces the interrupted one",
                ),
            )
            .await?;
        }
        OrphanedSession::NoneRecorded => {
            ctx.emit(
                Some(&node.id),
                degraded(
                    "restart_node — no session was recorded before the interruption; started \
                     fresh",
                ),
            )
            .await?;
        }
        OrphanedSession::NotAnOrphan => {}
    }
    Ok(None)
}

/// How many attempts this node has already announced.
async fn started_count(ctx: &RunCtx<'_>, node: &Node) -> Result<u32, RunError> {
    Ok(ctx
        .load_events()
        .await?
        .iter()
        .filter(|event| {
            event.node_id.as_ref() == Some(&node.id)
                && matches!(event.payload(), Some(EventPayload::NodeStarted(_)))
        })
        .count() as u32)
}

/// One session of a `kind: prompt` node. `repair` carries the previous
/// attempt's diagnostics when this is a repair.
async fn one_session(
    ctx: &RunCtx<'_>,
    node: &Node,
    prompt: &PromptSource,
    cancel: &CancellationToken,
    repair: Option<&str>,
) -> Result<Closed, RunError> {
    let rendered = match assemble_prompt(ctx, node, prompt, cancel, repair).await? {
        Step::Value(rendered) => rendered,
        Step::Ended(end) => return Ok(end.into()),
    };
    let chosen = match resolve_node_runner(ctx, node).await? {
        Step::Value(chosen) => chosen,
        Step::Ended(end) => return Ok(end.into()),
    };

    let adapter = &ctx.adapters[&chosen.adapter];
    report_declarative_network(ctx, node, adapter.as_ref(), &chosen.adapter).await?;
    // Names resolved by the engine; mounting is the adapter's —
    // and an adapter without the capability degrades with an event,
    // never a fatal error (a skill is instruction, not correctness).
    let skills = match crate::skills::resolve_skills(
        &ctx.manifest.config,
        &ctx.manifest.workflow,
        node,
        ctx.worktree,
    ) {
        Ok(skills) => skills,
        Err(error) => return Ok(fail(ctx, node, error.to_string(), false).await?.into()),
    };
    let skills = if !skills.is_empty()
        && !adapter
            .capabilities()
            .declares(yunta_core::Capability::Skills)
    {
        ctx.emit(
            Some(&node.id),
            EventPayload::CapabilityDegraded(yunta_core::events::CapabilityDegradedPayload {
                capability: yunta_core::Capability::Skills,
                adapter: chosen.adapter.clone(),
                policy_applied: "skills not mounted — the adapter declares no native \
                                 mechanism; the session runs without them"
                    .to_string(),
            }),
        )
        .await?;
        Vec::new()
    } else {
        skills
    };
    // A fresh listener + credential for THIS session attempt
    // when the adapter can be a client of it; `None` without the
    // capability is the resting state, not degradation — unless the
    // node sits in a `coordination: blackboard` group, whose declared
    // semantics the engine never emulates: that's a node failure.
    let run_tools = match open_run_tools(ctx, node, adapter.as_ref(), &chosen.adapter, None).await {
        Ok(resolution) => {
            if let Some(policy_applied) = resolution.degraded {
                ctx.emit(
                    Some(&node.id),
                    EventPayload::CapabilityDegraded(
                        yunta_core::events::CapabilityDegradedPayload {
                            capability: yunta_core::Capability::RunTools,
                            adapter: chosen.adapter.clone(),
                            policy_applied,
                        },
                    ),
                )
                .await?;
            }
            resolution.session
        }
        Err(error) => return Ok(fail(ctx, node, error.to_string(), false).await?.into()),
    };
    let request = SessionRequest {
        prompt: rendered,
        cwd: ctx.worktree.to_path_buf(),
        model: Some(chosen.model),
        agent: chosen.agent,
        permissions: session_profile(node),
        env: crate::task_cycle::SessionSetup::secrets_env(&ctx.manifest.config),
        edit_constraints: (!node.scope.is_empty()).then(|| node.scope.clone()),
        budget: ctx.session_budget().await?,
        adapter_settings: ctx.adapter_settings(&chosen.adapter),
        skills,
        run_tools_endpoint: run_tools.as_ref().map(|session| session.endpoint.clone()),
    };

    let resume_session = resume_target(ctx, node, adapter.as_ref(), &chosen.adapter).await?;

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
            close_node_staged(ctx, node, summary, tokens, &staged).await
        }
        DispatchOutcome::Failed { message, retryable } => {
            fail_with_tokens(ctx, node, message, retryable, tokens)
                .await
                .map(Closed::from)
        }
        // No terminal event means the engine synthesizes a retryable
        // failure — the adapter never invents one.
        DispatchOutcome::Crashed => fail_with_tokens(
            ctx,
            node,
            "session ended without a terminal event".to_string(),
            true,
            tokens,
        )
        .await
        .map(Closed::from),
        DispatchOutcome::BudgetExceeded { reason } => {
            fail_with_tokens(ctx, node, reason, false, tokens)
                .await
                .map(Closed::from)
        }
        DispatchOutcome::Cancelled => cancelled_end(ctx, node).await.map(Closed::from),
    }
}

/// What resume finds in the log for `node`: the id of a session
/// cut mid-flight (this dispatch is an orphan restart — a prior
/// `node_started` with no terminal event before the current one, and an
/// `agent_session_opened` inside that window), an orphan restart with no
/// session on record (crash before it opened), or nothing to resume at
/// all (a first attempt, or a retry after a *verdict* — a failed
/// session ended with an answer, only an interrupted one is continued).
enum OrphanedSession {
    Open(yunta_core::SessionId),
    NoneRecorded,
    NotAnOrphan,
}

fn orphaned_session(
    events: &[yunta_core::events::StoredEvent],
    node_id: &yunta_core::NodeId,
) -> OrphanedSession {
    let mine = |event: &&yunta_core::events::StoredEvent| event.node_id.as_ref() == Some(node_id);
    let starts: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            e.node_id.as_ref() == Some(node_id)
                && matches!(e.payload(), Some(EventPayload::NodeStarted(_)))
        })
        .map(|(i, _)| i)
        .collect();
    // The caller's own `node_started` for this attempt is already on the
    // log — the *previous* start is the one that may have been cut.
    let (Some(&current), Some(&previous)) = (
        starts.last(),
        starts.len().checked_sub(2).and_then(|i| starts.get(i)),
    ) else {
        return OrphanedSession::NotAnOrphan;
    };
    let Some(window) = events.get(previous..current) else {
        return OrphanedSession::NotAnOrphan;
    };
    let had_verdict = window.iter().filter(mine).any(|e| {
        matches!(
            e.payload(),
            Some(EventPayload::NodeFinished(_) | EventPayload::NodeFailed(_))
        )
    });
    if had_verdict {
        return OrphanedSession::NotAnOrphan;
    }
    match window
        .iter()
        .filter(mine)
        .rev()
        .find_map(|e| match e.payload() {
            Some(EventPayload::AgentSessionOpened(p)) => Some(p.session_id.clone()),
            _ => None,
        }) {
        Some(session_id) => OrphanedSession::Open(session_id),
        None => OrphanedSession::NoneRecorded,
    }
}
