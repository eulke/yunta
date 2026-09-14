//! One handler per terminal or continuing [`ScheduleStep`] — the bodies the
//! scheduler loop in [`super`] dispatches to, each named for the step it
//! serves so the loop reads as the schedule it runs.

use yunta_core::events::{
    EventPayload, Evidence, Fact, FindingSeverity, GateResolvedPayload, NodeReroutedPayload,
    PauseReason, PromotionSignaledPayload, RerouteCause, RerouteOrigin, RunFinishedPayload,
    TerminalState, TokenUsage,
};
use yunta_core::{ModeName, NodeId};

use crate::replay::RunState;
use crate::reserved::ReservedOption;
use crate::stats::tasks_done;

use super::{
    budget, escalation, find_node, gate_exec, node_close, node_exec, pause, questions_exec,
    schedule, RunCtx, RunError, RunReport, RunTerminal,
};
use yunta_core::events::{GateEvent, NodeEvent, RunEvent};

/// A corrupt log is exactly the one you most want exported — each event
/// serializes on its own, so a broken *sequence* doesn't stop the forensic
/// copy. Best-effort by design: if the export itself fails, the original
/// diagnostic wins, never masked by an IO error about its own post-mortem.
pub(super) async fn broken(ctx: &RunCtx<'_>, diagnostic: String) -> RunError {
    let diagnostic = match ctx.export_events_jsonl().await {
        Ok(()) => diagnostic,
        Err(export_error) => {
            format!("{diagnostic}; events.jsonl could not be exported: {export_error}")
        }
    };
    RunError::Broken { diagnostic }
}

/// Closes the run's log, whichever way it ends: the one `run_finished`
/// any ending writes, the forensic export that follows it, and — only at
/// a real `Done` — the worktree cleanup `on_finish` asked for. Returns
/// the state the close recorded, which is what every report carries.
///
/// One function because one close: three of them meant three places
/// that could each decide what a run costs and how many tasks it did.
pub(super) async fn finish(
    ctx: &RunCtx<'_>,
    terminal: TerminalState,
) -> Result<RunState, RunError> {
    let state = ctx.run_view().await?.state;
    ctx.emit(
        None,
        EventPayload::Run(RunEvent::Finished(RunFinishedPayload::closed(
            terminal,
            state.total_tokens(),
            tasks_done(&state),
        ))),
    )
    .await?;
    ctx.export_events_jsonl().await?;
    // `on_finish.cleanup: worktree` — after the export, only at a real Finish
    // (a paused run expects a resume in that tree; a promoted one seeds its
    // successor's worktree from it). A cleanup failure warns and never
    // un-finishes the run the log already closed.
    if terminal != TerminalState::Done {
        return Ok(state);
    }
    let wants_cleanup = ctx.manifest.workflow.on_finish.iter().any(|step| {
        matches!(
            step,
            yunta_core::OnFinishStep::Cleanup {
                cleanup: yunta_core::CleanupTarget::Worktree
            }
        )
    });
    if wants_cleanup && ctx.manifest.isolation == yunta_core::Isolation::Worktree {
        match crate::worktree::cleanup_worktree(
            ctx.worktree,
            &crate::worktree::run_branch(ctx.run_id),
            ctx.root_supervision(),
        )
        .await
        {
            Ok(crate::worktree::WorktreeCleanup::Removed) => {}
            Ok(crate::worktree::WorktreeCleanup::NotALinkedWorktree) => {
                ctx.engine_finding(
                    None,
                    "cleanup-not-a-worktree",
                    FindingSeverity::Minor,
                    "on_finish.cleanup: worktree skipped".to_string(),
                    ctx.worktree.display().to_string(),
                    "the run's tree is not a linked git worktree, so removing it would delete a \
                     primary checkout — nothing was touched"
                        .to_string(),
                )
                .await?;
            }
            Err(e) => {
                ctx.engine_finding(
                    None,
                    "cleanup-failed",
                    FindingSeverity::Minor,
                    "on_finish.cleanup: worktree failed".to_string(),
                    ctx.worktree.display().to_string(),
                    format!("the run's linked worktree could not be removed: {e}"),
                )
                .await?;
            }
        }
    }
    Ok(state)
}

/// Every node is done: distill what the run learned, then close it.
/// Distillation runs before the close because nothing is emitted after
/// `run_finished` and its findings are events.
pub(super) async fn run_finished(
    ctx: &RunCtx<'_>,
    mode_name: &ModeName,
) -> Result<RunReport, RunError> {
    super::distill::run_distill(ctx, mode_name).await?;
    Ok(RunReport {
        terminal: RunTerminal::Finished,
        state: finish(ctx, TerminalState::Done).await?,
    })
}

/// Closes the run as failed: a node failed and `defaults.on_failure`
/// (`abort`/`continue`) ended the run rather than pausing it. Nothing is
/// distilled and no worktree is cleaned up — a failed close is not a
/// real finish (D107), it expects no resume, and its knowledge is not
/// the attempt's knowledge to keep. The `node_failed` events already on
/// the log name every failed node; `reason` names one for the report.
pub(super) async fn run_failed(ctx: &RunCtx<'_>, reason: String) -> Result<RunReport, RunError> {
    Ok(RunReport {
        terminal: RunTerminal::Failed { reason },
        state: finish(ctx, TerminalState::Failed).await?,
    })
}

/// Records a node's re-route to another node, then lets the loop schedule the
/// target next.
pub(super) async fn reroute(
    ctx: &RunCtx<'_>,
    from: NodeId,
    to: NodeId,
    attempt: u32,
    max_reroutes: u32,
    cause: RerouteCause,
) -> Result<(), RunError> {
    ctx.emit(
        Some(&from),
        EventPayload::Node(NodeEvent::Rerouted(NodeReroutedPayload::new(
            to,
            cause,
            RerouteOrigin::OnFailure,
            Some(attempt),
            Some(max_reroutes),
        ))),
    )
    .await?;
    Ok(())
}

/// A node used up its re-routes to `goto`: the engine assembles the
/// escalation from the log (never the failed node, which has no further say)
/// and asks a human — or consumes a decision a `resolve_gate` MCP call
/// pre-seeded onto a parked log. `retry` re-routes once more (the loop
/// continues), `promote` closes the run into its next mode, and anything
/// else pauses. `None` means the loop continues; `Some` ends the run.
pub(super) async fn gate_exhausted(
    ctx: &RunCtx<'_>,
    state: &RunState,
    mode_name: &ModeName,
    node: NodeId,
    goto: NodeId,
    max_reroutes: u32,
    cause: RerouteCause,
) -> Result<Option<RunReport>, RunError> {
    let suggested_mode = schedule::next_mode_after(&ctx.manifest.workflow, mode_name);
    let escalation = escalation::build_reroute_escalation(
        &ctx.manifest.workflow,
        mode_name,
        &node,
        &goto,
        max_reroutes,
        &cause,
    )
    .map_err(|source| RunError::Broken {
        diagnostic: format!("node `{node}`'s escalation: {source}"),
    })?;
    // A decision `resolve_gate` pre-seeded onto the log while this run was
    // parked is consumed here, by this same consequence code — never
    // re-asked, and its escalation pair is already recorded so it is never
    // re-emitted. The option is re-validated against the re-derived menu: a
    // mismatch means ask normally.
    let pre_seeded = escalation::pre_seeded_resolution(state, &node, &escalation);
    let already_recorded = pre_seeded.is_some();
    let choice = match pre_seeded {
        Some(choice) => Some(choice),
        None => ctx.ask_human(&escalation).await?,
    };
    let Some(choice) = choice else {
        // No live surface to ask (headless, no TTY, `yunta test`): pause and
        // let a later `yunta resume` (or a future MCP client) carry the
        // decision instead.
        return Ok(Some(
            pause(ctx, PauseReason::Escalation(Box::new(escalation.clone()))).await?,
        ));
    };
    if !already_recorded {
        ctx.emit(
            Some(&node),
            EventPayload::Gates(GateEvent::Waiting(escalation.clone().into_payload())),
        )
        .await?;
        ctx.emit(
            Some(&node),
            EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(
                choice.clone(),
            ))),
        )
        .await?;
    }
    let chosen = ReservedOption::of(&choice.option);
    if chosen == Some(ReservedOption::Retry) {
        ctx.emit(
            Some(&node),
            EventPayload::Node(NodeEvent::Rerouted(NodeReroutedPayload::new(
                goto,
                cause,
                RerouteOrigin::OnFailure,
                Some(max_reroutes + 1),
                Some(max_reroutes),
            ))),
        )
        .await?;
        Ok(None)
    } else if chosen == Some(ReservedOption::Promote) {
        // `"promote"` only ever appears as an option when a successor
        // mode was carried with it; a run whose log offers `promote`
        // without one is corrupt, reported rather than unwrapped.
        let Some(next_mode) = suggested_mode else {
            return Err(RunError::Broken {
                diagnostic: format!(
                    "node `{node}` chose `promote` but its gate carried no successor mode"
                ),
            });
        };
        let evidence: Evidence = vec![Fact::bare(cause.to_string())].into();
        ctx.emit(
            None,
            EventPayload::Run(RunEvent::PromotionSignaled(PromotionSignaledPayload {
                reason: yunta_core::text::aside(
                    format!("node `{node}` exhausted its re-routes to `{goto}`"),
                    &evidence.one_line(),
                ),
                evidence,
                suggested_mode: next_mode.clone(),
            })),
        )
        .await?;
        // A promotion is a real close — the short attempt's knowledge is
        // knowledge, and the successor inherits it through the repo layer.
        super::distill::run_distill(ctx, mode_name).await?;
        // Findings without an artifact (scope-expansion denials, for
        // instance) live only on this log — derive them into an inheritable
        // artifact so the successor's copied context carries them. No
        // findings, no file.
        let events_for_close = ctx.load_events().await?;
        let inherited = crate::findings::inherited_findings(&events_for_close);
        if !inherited.is_empty() {
            let file = yunta_core::FindingsFile::from_findings(inherited);
            let yaml = yunta_core::yaml::to_string(&file).map_err(|e| RunError::Broken {
                diagnostic: format!("failed to serialize inherited findings: {e}"),
            })?;
            // The run's own artifact, not any node's: it is what this
            // log adds up to, and the successor inherits it as it does
            // every other artifact this run holds.
            crate::artifacts::accept(
                &ctx.log(),
                ctx.run_dir,
                None,
                yunta_core::events::ArtifactId::Interpreted {
                    kind: yunta_core::ArtifactKind::Findings,
                },
                yaml.as_bytes(),
                yunta_core::events::RecordedOrigin::Derived,
            )
            .await?;
        }
        Ok(Some(RunReport {
            terminal: RunTerminal::Promoted {
                suggested_mode: next_mode,
            },
            state: finish(ctx, TerminalState::Promoted).await?,
        }))
    } else {
        // The menu offers nothing beyond retry, promote and abort.
        Ok(Some(
            pause(
                ctx,
                PauseReason::GateAborted {
                    node: node.clone(),
                    free_text: choice.free_text.clone(),
                },
            )
            .await?,
        ))
    }
}

/// A batch of independently-ready nodes to run together. The run-budget check
/// guards exactly this token-spending step; the batch then runs to completion
/// (every member reaches a terminal per-node state) before the next iteration
/// decides what comes next. `Some` ends the run — an exhausted budget with no
/// surface to lift it, or a child run that paused; `None` continues the loop.
pub(super) async fn execute_batch(
    ctx: &RunCtx<'_>,
    state: &RunState,
    batch: Vec<(NodeId, u32)>,
) -> Result<Option<RunReport>, RunError> {
    // The budget check guards exactly the steps that spend tokens — a run
    // whose remaining work is gates and questions finishes without ever
    // tripping it.
    if !ctx.budget_lifted.load(std::sync::atomic::Ordering::Relaxed) {
        if let Some(cap) = ctx
            .manifest
            .config
            .limits
            .as_ref()
            .and_then(|limits| limits.max_tokens_per_run)
        {
            let spent = state.total_tokens().total();
            if spent >= cap {
                let (escalation, reason) = budget::over_budget_escalation(ctx, spent, cap)
                    .map_err(|source| RunError::Broken {
                        diagnostic: format!("the run's budget escalation: {source}"),
                    })?;
                match budget::escalate(ctx, None, escalation, reason).await? {
                    budget::BudgetDecision::Continue => ctx
                        .budget_lifted
                        .store(true, std::sync::atomic::Ordering::Relaxed),
                    budget::BudgetDecision::Pause { reason } => {
                        return Ok(Some(pause(ctx, reason).await?));
                    }
                }
            }
        }
    }
    // Top-level DAG fan-out never interrupts a still-running sibling the
    // moment one fails — that's `join: any`'s own semantics, scoped to a
    // named `parallel` group, not implicit `max_parallel_nodes` batches — so
    // each node gets a token nothing ever cancels.
    let cancel_for_batch = ctx.root_cancel.child_token();
    let executions = batch.into_iter().map(|(node_id, attempt)| {
        let cancel_for_batch = cancel_for_batch.clone();
        async move {
            let node = find_node(&ctx.manifest.workflow, &node_id)?;
            node_exec::execute_node(ctx, node, attempt, &cancel_for_batch).await
        }
    });
    // A workflow node whose child run paused can't close its node (the parent
    // waits on the child's *terminal* state) — after the whole batch lands,
    // the parent pauses too, naming the child. A root cancellation takes
    // precedence: the loop-top check handles it as "cancelled by user".
    let mut child_paused: Option<(NodeId, String)> = None;
    for result in futures::future::join_all(executions).await {
        if let node_exec::NodeEnd::ChildPaused { node, reason } = result? {
            child_paused.get_or_insert((node, reason));
        }
    }
    if let Some((node, reason)) = child_paused {
        if !ctx.root_cancel.is_cancelled() {
            return Ok(Some(
                pause(ctx, PauseReason::ChildPaused { node, reason }).await?,
            ));
        }
    }
    Ok(None)
}

/// Publishes a `kind: gate` node's external artifacts (opening a PR, or
/// degrading to console without a forge) and pauses the run until it is
/// resolved. `Some` when the gate is still waiting — the pause the forge
/// round-trip already recorded; `None` once it has resolved.
pub(super) async fn publish_gate(
    ctx: &RunCtx<'_>,
    node_id: NodeId,
) -> Result<Option<RunReport>, RunError> {
    let node = find_node(&ctx.manifest.workflow, &node_id)?;
    let yunta_core::NodeKind::Gate {
        assignee,
        external: Some(external),
        ..
    } = &node.kind
    else {
        return Err(RunError::Broken {
            diagnostic: format!(
                "scheduler chose node `{}` as an external gate to publish, but it isn't one",
                node.id
            ),
        });
    };
    let step = gate_exec::publish_gate(ctx, node, assignee, external, ctx.forge).await?;
    gate_still_waiting(ctx, step).await
}

/// Polls a published gate's forge and maps its verdict — approved,
/// changes-requested, closed, or still pending. `Some` while still waiting.
pub(super) async fn poll_gate(
    ctx: &RunCtx<'_>,
    node_id: NodeId,
    external_ref: String,
) -> Result<Option<RunReport>, RunError> {
    let node = find_node(&ctx.manifest.workflow, &node_id)?;
    let step = gate_exec::poll_gate(ctx, node, &external_ref, ctx.forge).await?;
    gate_still_waiting(ctx, step).await
}

/// Resolves an internal gate (`external: None`) by putting its
/// `message`/`options`/`on` to a human. `Some` while still waiting.
pub(super) async fn resolve_internal_gate(
    ctx: &RunCtx<'_>,
    node_id: NodeId,
) -> Result<Option<RunReport>, RunError> {
    let node = find_node(&ctx.manifest.workflow, &node_id)?;
    let yunta_core::NodeKind::Gate {
        assignee,
        message,
        options,
        on,
        external: None,
    } = &node.kind
    else {
        return Err(RunError::Broken {
            diagnostic: format!(
                "scheduler chose node `{}` as an internal gate, but it isn't one",
                node.id
            ),
        });
    };
    gate_still_waiting(
        ctx,
        gate_exec::resolve_internal_gate(ctx, node, assignee, message.as_deref(), options, on)
            .await?,
    )
    .await
}

/// Puts a `kind: questions` node's unanswered questions to a human. `Some`
/// when no surface can answer and the run pauses.
pub(super) async fn ask_questions(
    ctx: &RunCtx<'_>,
    node_id: NodeId,
) -> Result<Option<RunReport>, RunError> {
    let node = find_node(&ctx.manifest.workflow, &node_id)?;
    match questions_exec::execute_ask(ctx, node).await? {
        questions_exec::AskOutcome::Answered => Ok(None),
        questions_exec::AskOutcome::Pause { reason } => Ok(Some(pause(ctx, reason).await?)),
    }
}

/// Pays a node the terminal its close deferred: its questions were
/// answered, and the `node_finished` is all that is left.
///
/// No session opens and no attempt starts. The node closed when it
/// asked — its hooks ran, its diff was audited, its artifacts were
/// verified — so a resume that lands here after a crash costs an append,
/// not a session.
pub(super) async fn finish_answered(
    ctx: &RunCtx<'_>,
    node_id: NodeId,
) -> Result<Option<RunReport>, RunError> {
    let node = find_node(&ctx.manifest.workflow, &node_id)?;
    node_close::finish_node(ctx, node, "questions answered", TokenUsage::default()).await?;
    Ok(None)
}

/// What a gate's own answer does to the run. The gate decides that it
/// waits and why; the run is what writes `run_paused`, here and in no
/// other place — three gate shapes, one pause.
async fn gate_still_waiting(
    ctx: &RunCtx<'_>,
    step: gate_exec::GateStep,
) -> Result<Option<RunReport>, RunError> {
    match step {
        gate_exec::GateStep::Waiting(reason) => Ok(Some(pause(ctx, reason).await?)),
        gate_exec::GateStep::Resolved => Ok(None),
    }
}
