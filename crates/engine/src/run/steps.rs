//! One handler per terminal or continuing [`Decision`](super::schedule::Decision) — the bodies the
//! scheduler loop in [`super::exec`] dispatches to, each named for the step it
//! serves so the loop reads as the schedule it runs.

use chrono::{DateTime, Utc};
use yunta_core::events::{
    EngineFact, EventPayload, Evidence, Failure, NodeOutcome, NodeReroutedPayload, Outlook,
    PauseReason, PromotionCause, PromotionSignaledPayload, RerouteOrigin, RunFinishedPayload,
    TerminalState, TokenUsage,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::{ModeName, NodeId};

use crate::replay::RunState;

use super::{
    budget, escalation, find_node, gate_exec, node_close, node_exec, pause, questions_exec,
    schedule, RunCtx, RunError, RunReport, RunTerminal,
};
use yunta_core::events::{NodeEvent, RunEvent};

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
/// any ending writes, the cleanup that follows it — and, only at a real
/// `Done`, the worktree cleanup `on_finish` asked for — then the forensic
/// export. Returns the state of the closed log, the close and any finding
/// its cleanup recorded included, which is what every report carries: a
/// paused run's report holds its pause the same way.
///
/// One function because one close: three of them meant three places
/// that could each decide what a run costs and how many tasks it did.
pub(super) async fn finish(
    ctx: &RunCtx<'_>,
    terminal: TerminalState,
) -> Result<RunState, RunError> {
    let open = ctx.run_view().await?.state;
    let closed = RunFinishedPayload::closed(terminal, open.metrics());
    ctx.emit(None, EventPayload::Run(RunEvent::Finished(closed)))
        .await?;
    super::cleanup::units(ctx).await;
    if terminal != TerminalState::Promoted {
        super::cleanup::run(ctx).await;
    }
    // `on_finish.cleanup: worktree`, only at a real Finish: a paused run
    // expects a resume in that tree, and a promoted one seeds its
    // successor's worktree from it. A cleanup that cannot run is a finding
    // on the closed log and never un-finishes the run.
    if terminal == TerminalState::Done {
        super::cleanup::worktrees(ctx).await?;
    }
    // One read once the log is whole: the export and the report hold it.
    let view = ctx.run_view().await?;
    ctx.export(&view.events)?;
    Ok(*view.state)
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
/// real finish, it expects no resume, and its knowledge is not
/// the attempt's knowledge to keep. The `node_failed` events already on
/// the log name every failed node; `node` and `failure` name the one that
/// ended the run, for the report.
pub(super) async fn run_failed(
    ctx: &RunCtx<'_>,
    node: NodeId,
    failure: Failure,
) -> Result<RunReport, RunError> {
    Ok(RunReport {
        terminal: RunTerminal::Failed { node, failure },
        state: finish(ctx, TerminalState::Failed).await?,
    })
}

/// A node failed on a limit that resets at `at`: the run pauses awaiting that
/// reset. The detached resume that follows waits for it before the run wakes.
pub(super) async fn pause_for_reset(
    ctx: &RunCtx<'_>,
    node: NodeId,
    at: DateTime<Utc>,
) -> Result<RunReport, RunError> {
    pause(ctx, PauseReason::AwaitingReset { node, at }).await
}

/// Records a node's re-route to another node, then lets the loop schedule the
/// target next.
pub(super) async fn reroute(
    ctx: &RunCtx<'_>,
    from: NodeId,
    to: NodeId,
    attempt: u32,
    max_reroutes: u32,
    cause: Failure,
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

/// A failed node with no re-route of its own, put to a person: a choice
/// that runs it again — `retry`, `continue-work`, `grant` — is recorded
/// and the loop continues, where the scheduler starts the node's next
/// attempt and that attempt applies what was chosen; `abort`, or nobody
/// to ask, pauses on the failure
/// itself, so every surface still names it and a resume asks again.
pub(super) async fn failure_escalation(
    ctx: &RunCtx<'_>,
    state: &RunState,
    node: NodeId,
    failure: Failure,
    next_attempt: u32,
    outlook: Outlook,
) -> Result<Option<RunReport>, RunError> {
    let escalation = escalation::build_failure_escalation(&node, &failure, next_attempt, &outlook);
    let answer = escalation::decided(ctx, state, &node, None, &escalation).await?;
    if answer.is_some_and(|(choice, _)| {
        ReservedOption::of(&choice.option).is_some_and(ReservedOption::runs_again)
    }) {
        return Ok(None);
    }
    Ok(Some(
        pause(ctx, PauseReason::NodeFailed { node, failure }).await?,
    ))
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
    cause: Failure,
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
    let Some((choice, _)) = escalation::decided(ctx, state, &node, None, &escalation).await? else {
        // No live surface to ask (headless, no TTY, `yunta test`): pause and
        // let a later `yunta resume` (or an MCP client) carry the
        // decision instead.
        return Ok(Some(
            pause(ctx, PauseReason::Escalation(Box::new(escalation.clone()))).await?,
        ));
    };
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
        promote_exhausted(ctx, mode_name, node, goto, cause, suggested_mode).await
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

/// The `promote` choice of an exhausted gate: the run closes into the
/// successor mode its gate carried, after distilling what it learned.
async fn promote_exhausted(
    ctx: &RunCtx<'_>,
    mode_name: &ModeName,
    node: NodeId,
    goto: NodeId,
    cause: Failure,
    suggested_mode: Option<ModeName>,
) -> Result<Option<RunReport>, RunError> {
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
        EventPayload::Run(RunEvent::PromotionSignaled(PromotionSignaledPayload::new(
            PromotionCause {
                node,
                goto,
                failure: Some(cause),
            },
            Evidence::none(),
            next_mode.clone(),
        ))),
    )
    .await?;
    // A promotion is a real close — the short attempt's knowledge is
    // knowledge, and the successor inherits it through the repo layer.
    super::distill::run_distill(ctx, mode_name).await?;
    carry_findings(ctx).await?;
    Ok(Some(RunReport {
        terminal: RunTerminal::Promoted {
            suggested_mode: next_mode,
        },
        state: finish(ctx, TerminalState::Promoted).await?,
    }))
}

/// Findings without an artifact (scope-expansion denials, for instance)
/// live only on this log — derive them into an inheritable artifact so the
/// successor's copied context carries them. No findings, no file.
async fn carry_findings(ctx: &RunCtx<'_>) -> Result<(), RunError> {
    let events_for_close = ctx.load_events().await?;
    let inherited = crate::findings::inherited_findings(&events_for_close);
    if !inherited.is_empty() {
        // The one door a findings document is rendered through, so
        // the run's own is held to `limits.max_artifact_bytes` like
        // every other artifact it accepts. The run's artifact, not
        // any node's: it is what this log adds up to, and the
        // successor inherits it as it does every other one.
        let derived = crate::artifacts::derive_findings(
            None,
            inherited,
            ctx.manifest
                .config
                .limits
                .as_ref()
                .and_then(|limits| limits.max_artifact_bytes),
        )
        .map_err(|error| RunError::Broken {
            diagnostic: error.to_string(),
        })?;
        crate::artifacts::accept(
            &ctx.log(),
            ctx.run_dir,
            None,
            derived.artifact.clone(),
            &derived.bytes,
            yunta_core::events::RecordedOrigin::Derived,
        )
        .await?;
    }
    Ok(())
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
    // Work needs the disk's floor free: a batch that would start below it
    // stops the run instead, saying how to make room.
    Box::pin(ctx.pool.keeps_its_floor(ctx.root_supervision())).await?;
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
            let spent = state.spend().so_far().total();
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
    // A person who chose to run a node again after its failure says the
    // world it failed in may have changed: every red the run holds is
    // asked again, and what passed stays passed.
    if batch
        .iter()
        .any(|(node, _)| state.run_again_chosen(node).is_some())
    {
        ctx.memo.forget_failures();
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
    // A node whose run paused — a child run on its own log, or a departure the
    // person aborted — can't close its node: after the whole batch lands, the
    // run pauses too, with the first such reason. A root cancellation takes
    // precedence: the loop-top check handles it as "cancelled by user".
    let mut paused: Option<PauseReason> = None;
    for result in futures::future::join_all(executions).await {
        if let node_exec::NodeEnd::Paused { reason } = result? {
            paused.get_or_insert(reason);
        }
    }
    if let Some(reason) = paused {
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
        message,
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
    let step =
        gate_exec::publish_gate(ctx, node, assignee, message.as_deref(), external, ctx.forge)
            .await?;
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

/// Resolves an internal gate (`external: None`) by putting it to a
/// person. `Some` while still waiting.
pub(super) async fn resolve_internal_gate(
    ctx: &RunCtx<'_>,
    node_id: NodeId,
) -> Result<Option<RunReport>, RunError> {
    let node = find_node(&ctx.manifest.workflow, &node_id)?;
    gate_still_waiting(ctx, super::internal_gate::resolve(ctx, node).await?).await
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
    node_close::finish_node(
        ctx,
        node,
        NodeOutcome::Fact(EngineFact::QuestionsAnswered),
        TokenUsage::default(),
    )
    .await?;
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
