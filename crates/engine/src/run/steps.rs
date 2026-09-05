//! One handler per terminal or continuing [`ScheduleStep`] — the bodies the
//! scheduler loop in [`super`] dispatches to, each named for the step it
//! serves so the loop reads as the schedule it runs.

use yunta_core::events::{
    EventPayload, FindingSeverity, GateResolvedPayload, NodeReroutedPayload,
    PromotionSignaledPayload, RerouteOrigin, RunFinishedPayload, RunMetrics, StoredEvent,
    TerminalState,
};
use yunta_core::{ModeName, NodeId};

use crate::replay::derive;
use crate::reserved::ReservedOption;
use crate::stats::cptv;

use super::{
    budget, escalation, find_node, gate_exec, node_exec, pause, questions_exec, schedule, RunCtx,
    RunError, RunReport, RunTerminal,
};

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

/// Every node is done: distill, close the log with `run_finished`, export,
/// and clean up the worktree if the workflow asked — then report the run
/// finished.
pub(super) async fn finish(ctx: &RunCtx<'_>, mode_name: &ModeName) -> Result<RunReport, RunError> {
    // Distill before `run_finished` — nothing is emitted after the close
    // event, and its findings are events.
    super::distill::run_distill(ctx, mode_name).await?;
    let state = ctx.run_view().await?.state;
    ctx.emit(
        None,
        EventPayload::RunFinished(RunFinishedPayload {
            terminal_state: TerminalState::Done,
            metrics: RunMetrics {
                cptv: cptv(&state),
                tokens: state.total_tokens,
            },
        }),
    )
    .await?;
    ctx.export_events_jsonl().await?;
    // `on_finish.cleanup: worktree` — after the export, only at a real Finish
    // (a paused run expects a resume in that tree; a promoted one seeds its
    // successor's worktree from it). A cleanup failure warns and never
    // un-finishes the run the log already closed.
    let wants_cleanup = ctx.manifest.workflow.on_finish.iter().any(|step| {
        matches!(
            step,
            yunta_core::OnFinishStep::Cleanup {
                cleanup: yunta_core::CleanupTarget::Worktree
            }
        )
    });
    if wants_cleanup && ctx.manifest.isolation == yunta_core::Isolation::Worktree {
        match crate::worktree::cleanup_worktree(ctx.worktree, &format!("yunta/{}", ctx.run_id))
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
    Ok(RunReport {
        terminal: RunTerminal::Finished,
        state,
    })
}

/// Closes the run as failed: a node failed and `defaults.on_failure`
/// (`abort`/`continue`) ended the run rather than pausing it. Unlike
/// [`finish`], nothing is distilled and no worktree is cleaned up — a
/// failed close is not a real finish (D107), it expects no resume, and
/// its knowledge is not the attempt's knowledge to keep. The `node_failed`
/// events already on the log name every failed node; `reason` names one
/// for the report.
pub(super) async fn run_failed(ctx: &RunCtx<'_>, reason: String) -> Result<RunReport, RunError> {
    let state = ctx.run_view().await?.state;
    ctx.emit(
        None,
        EventPayload::RunFinished(RunFinishedPayload {
            terminal_state: TerminalState::Failed,
            metrics: RunMetrics {
                cptv: cptv(&state),
                tokens: state.total_tokens,
            },
        }),
    )
    .await?;
    ctx.export_events_jsonl().await?;
    Ok(RunReport {
        terminal: RunTerminal::Failed { reason },
        state,
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
    cause: String,
) -> Result<(), RunError> {
    ctx.emit(
        Some(&from),
        EventPayload::NodeRerouted(NodeReroutedPayload {
            to_node: to,
            cause,
            attempt: Some(attempt),
            max_reroutes: Some(max_reroutes),
            origin: RerouteOrigin::OnFailure,
        }),
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
    events: &[StoredEvent],
    mode_name: &ModeName,
    node: NodeId,
    goto: NodeId,
    max_reroutes: u32,
    cause: String,
) -> Result<Option<RunReport>, RunError> {
    let suggested_mode = schedule::next_mode_after(&ctx.manifest.workflow, mode_name);
    let escalation = escalation::build_reroute_escalation(
        &ctx.manifest.workflow,
        mode_name,
        &node,
        &goto,
        max_reroutes,
        &cause,
    );
    // A decision `resolve_gate` pre-seeded onto the log while this run was
    // parked is consumed here, by this same consequence code — never
    // re-asked, and its escalation pair is already recorded so it is never
    // re-emitted. The option is re-validated against the re-derived menu: a
    // mismatch means ask normally.
    let pre_seeded = escalation::pre_seeded_resolution(events, &node, &escalation);
    let already_recorded = pre_seeded.is_some();
    let choice = match pre_seeded {
        Some(choice) => Some(choice),
        None => ctx.ask_human(&escalation).await?,
    };
    let Some(choice) = choice else {
        // No live surface to ask (headless, no TTY, `yunta test`): pause and
        // let a later `yunta resume` (or a future MCP client) carry the
        // decision instead.
        return Ok(Some(pause(ctx, escalation.summary).await?));
    };
    if !already_recorded {
        ctx.emit(Some(&node), EventPayload::GateWaiting(escalation))
            .await?;
        ctx.emit(
            Some(&node),
            EventPayload::GateResolved(GateResolvedPayload::Chosen(choice.clone())),
        )
        .await?;
    }
    let chosen = ReservedOption::of(&choice.option);
    if chosen == Some(ReservedOption::Retry) {
        ctx.emit(
            Some(&node),
            EventPayload::NodeRerouted(NodeReroutedPayload {
                to_node: goto,
                cause,
                attempt: Some(max_reroutes + 1),
                max_reroutes: Some(max_reroutes),
                origin: RerouteOrigin::OnFailure,
            }),
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
        ctx.emit(
            None,
            EventPayload::PromotionSignaled(PromotionSignaledPayload {
                reason: format!("node `{node}` exhausted its re-routes to `{goto}`: {cause}"),
                evidence: cause,
                suggested_mode: next_mode.clone(),
            }),
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
            let path = ctx.run_dir.join("artifacts/findings-inherited.yaml");
            std::fs::write(&path, yaml).map_err(|source| RunError::Io {
                context: format!("write `{}`", path.display()),
                source,
            })?;
        }
        let state = derive(&events_for_close);
        ctx.emit(
            None,
            EventPayload::RunFinished(RunFinishedPayload {
                terminal_state: TerminalState::Promoted,
                metrics: RunMetrics {
                    cptv: cptv(&state),
                    tokens: state.total_tokens,
                },
            }),
        )
        .await?;
        ctx.export_events_jsonl().await?;
        Ok(Some(RunReport {
            terminal: RunTerminal::Promoted {
                suggested_mode: next_mode,
            },
            state: ctx.run_view().await?.state,
        }))
    } else {
        // The menu offers nothing beyond retry, promote and abort.
        let reason = format!(
            "node `{node}`'s gate was resolved to abort{}",
            choice
                .free_text
                .as_deref()
                .map(|text| format!(": {text}"))
                .unwrap_or_default()
        );
        Ok(Some(pause(ctx, reason).await?))
    }
}

/// A batch of independently-ready nodes to run together. The run-budget check
/// guards exactly this token-spending step; the batch then runs to completion
/// (every member reaches a terminal per-node state) before the next iteration
/// decides what comes next. `Some` ends the run — an exhausted budget with no
/// surface to lift it, or a child run that paused; `None` continues the loop.
pub(super) async fn execute_batch(
    ctx: &RunCtx<'_>,
    events: &[StoredEvent],
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
            let spent = derive(events).total_tokens.total();
            if spent >= cap {
                let (escalation, reason) = budget::over_budget_escalation(ctx, spent, cap);
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
    let mut child_paused: Option<String> = None;
    for result in futures::future::join_all(executions).await {
        if let node_exec::NodeEnd::ChildPaused { reason } = result? {
            child_paused.get_or_insert(reason);
        }
    }
    if let Some(reason) = child_paused {
        if !ctx.root_cancel.is_cancelled() {
            return Ok(Some(pause(ctx, reason).await?));
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
    let step =
        gate_exec::resolve_internal_gate(ctx, node, assignee, message.as_deref(), options, on)
            .await?;
    if let gate_exec::GateStep::StillWaiting { reason } = step {
        return Ok(Some(pause(ctx, reason).await?));
    }
    Ok(None)
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

/// A published/polled gate that is still waiting has already recorded its
/// pause through the forge round-trip, so the run only needs its paused
/// report — never a second `run_paused`.
async fn gate_still_waiting(
    ctx: &RunCtx<'_>,
    step: gate_exec::GateStep,
) -> Result<Option<RunReport>, RunError> {
    match step {
        gate_exec::GateStep::StillWaiting { reason } => Ok(Some(RunReport {
            terminal: RunTerminal::Paused { reason },
            state: ctx.run_view().await?.state,
        })),
        gate_exec::GateStep::Resolved => Ok(None),
    }
}
