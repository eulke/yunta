//! Settling one batch member: what its attempts left for the log, the outcome
//! of its cycle put there, and its checkout given back to the pool.

use tokio_util::sync::CancellationToken;
use yunta_core::events::{
    EventPayload, TaskCause, TaskEvent, TaskLedger, TaskStatus, TaskStatusChangedPayload,
};
use yunta_core::expansion::Decision;
use yunta_core::{Node, Seq, Task};

use crate::run::scope_record::{self, Withdrawal};
use crate::run::{RunCtx, RunError};
use crate::scope_expansion::{ScopeExpansionOutcome, ScopeExpansionRequest};
use crate::task_cycle::{AttemptRecord, TaskOutcome};
use crate::worktree::Lease;

use super::super::depart::{self, PendingDeparture};
use super::super::dispatch::{left_work, Dispatched};
use super::super::escalate::PendingEscalation;
use super::super::LoopState;
use super::{integrate_task, record_attempt, IntegrationOutcome, VerifiedTask};

/// What settling one batch member leaves for the walk.
pub(super) enum Settled {
    /// The member is on the log, with what it owes a person once the batch is in.
    Done {
        escalation: Option<PendingEscalation>,
        departure: Option<PendingDeparture>,
    },
    /// The member's session was interrupted, and the walk ends at it. The hold
    /// on its checkout stands until the node has ended.
    Interrupted(Lease),
}

/// Settles one batch member: records what its attempts left for the log, puts
/// the outcome of its cycle there, and gives its checkout back unless the cycle
/// was interrupted.
pub(super) async fn settle_member(
    ctx: &RunCtx<'_>,
    node: &Node,
    state: &mut LoopState,
    owed: &TaskLedger,
    cancel: &CancellationToken,
    dispatched: Dispatched<'_>,
) -> Result<Settled, RunError> {
    // The checkout stays held until this task is integrated or set aside
    // below, and is given back then.
    let Dispatched {
        task,
        unit,
        lease,
        mut report,
    } = dispatched;
    if let Some(cause) = depart::owed_first(owed, &report) {
        report.outcome = TaskOutcome::Blocked { cause };
    }
    let needs_human_decision = report.needs_human_decision;
    // The cycle recorded each of its checks the moment it ran; what
    // it hands over is where the last one landed, which the status
    // change closing the cycle cites.
    let mut last_check_seq = report.last_check.ok_or_else(|| RunError::Broken {
        diagnostic: format!("task `{}`'s cycle recorded no check", task.id),
    })?;
    let escalated = record_attempts(ctx, node, task, state, &mut report.attempts).await?;
    // A cancelled dispatch ends the whole loop node without a verdict —
    // the task stays `running` in the log (orphaned), which is exactly
    // what makes a later resume re-execute it — unless a session still to
    // come could not be stopped, which fails the node instead.
    if matches!(report.outcome, TaskOutcome::Interrupted) {
        return Ok(Settled::Interrupted(lease));
    }
    let member = VerifiedTask {
        task,
        unit: &unit,
        lease: &lease,
        staged: &report.staged,
    };
    let blocked = conclude(
        ctx,
        node,
        member,
        report.outcome,
        &mut last_check_seq,
        cancel,
    )
    .await?;
    ctx.pool.give_back(lease, ctx.supervision(cancel)).await?;
    Ok(owed_of(task, escalated, needs_human_decision, blocked))
}

/// Records what each of a member's attempts left that the cycle could not
/// record itself, and returns the request the last attempt that escalated one
/// left, with that attempt's number. The escalated request itself (paths,
/// reason, criterion and its pre-check exit), captured off the attempt that
/// raised it, is what the escalation object is built from.
async fn record_attempts(
    ctx: &RunCtx<'_>,
    node: &Node,
    task: &Task,
    state: &mut LoopState,
    attempts: &mut Vec<AttemptRecord>,
) -> Result<Option<(u32, ScopeExpansionOutcome)>, RunError> {
    let mut escalated: Option<(u32, ScopeExpansionOutcome)> = None;
    // The request the attempt before this one decided, if it made one: a
    // fresh session re-reads a request still pending, and the log holds it once.
    let mut previous_asked: Option<ScopeExpansionRequest> = None;
    for attempt in attempts.drain(..) {
        state.tokens += attempt.tokens;
        // `run_task` has no ctx to record on, so it hands the
        // breach here, after the check that found it.
        if let Some(breach) = &attempt.fence_breach {
            ctx.record_breach(&node.id, breach).await?;
        }

        if let Some(outcome) = &attempt.scope_expansion {
            let new_request = outcome.asked.is_some() && outcome.asked != previous_asked;
            record_attempt(ctx, node, task, attempt.attempt, outcome, new_request).await?;
            if matches!(outcome.decision, Decision::Escalate { .. }) {
                escalated = Some((attempt.attempt, outcome.clone()));
            }
        }
        previous_asked = attempt
            .scope_expansion
            .as_ref()
            .and_then(|outcome| outcome.asked.clone());
        // A request the task closed with, unanswered, is withdrawn: nobody
        // decided it, so the log says it left with the task.
        if let Some(paths) = &attempt.withdrawn {
            let request = Withdrawal {
                task: &task.id,
                paths,
            };
            scope_record::withdraw(&ctx.log(), &node.id, request).await?;
        }
    }
    Ok(escalated)
}

/// Puts a member's outcome on the log: the block its cycle ended on, or the
/// integration of a task its cycle verified done. Returns the cause the member
/// blocked on, when it did.
async fn conclude(
    ctx: &RunCtx<'_>,
    node: &Node,
    member: VerifiedTask<'_>,
    outcome: TaskOutcome,
    last_check_seq: &mut Seq,
    cancel: &CancellationToken,
) -> Result<Option<TaskCause>, RunError> {
    let task = member.task;
    match outcome {
        TaskOutcome::Blocked { cause } => {
            let left_work = left_work(ctx, node, task, member.unit, cancel).await?;
            ctx.emit(
                Some(&node.id),
                EventPayload::Tasks(TaskEvent::StatusChanged(TaskStatusChangedPayload::blocked(
                    task.id.clone(),
                    *last_check_seq,
                    left_work,
                    cause.clone(),
                ))),
            )
            .await?;
            Ok(Some(cause))
        }
        TaskOutcome::Done => {
            integrate_done(ctx, node, member, last_check_seq, cancel).await?;
            // Never counted toward the loop's own "no task ready"
            // diagnostic — a `Pending` task is retriable, not stuck, so
            // there's nothing to cite a human decision for yet.
            Ok(None)
        }
        // Handled by the early return above.
        TaskOutcome::Interrupted => Ok(None),
    }
}

/// Integrates a task its cycle verified done, and records the status the
/// integration leaves: done at the commit its work landed at, or back to
/// `Pending` when the rebase or the re-verification rejected it.
async fn integrate_done(
    ctx: &RunCtx<'_>,
    node: &Node,
    member: VerifiedTask<'_>,
    last_check_seq: &mut Seq,
    cancel: &CancellationToken,
) -> Result<(), RunError> {
    let task = member.task;
    let outcome = integrate_task(ctx, node, member, &ctx.memo, last_check_seq, cancel).await?;
    // A rejected integration goes back to ready on the new tree —
    // back to `Pending`, where batch selection takes it up again. This is
    // deliberately NOT `Blocked`: green in isolation but broken by a
    // sibling's integration is a timing artifact of concurrency, not
    // evidence the task itself can't succeed — that verdict only comes from
    // `run_task`'s own retry exhaustion, the branch above. The
    // `task_status_changed` emitted just below is the log's record of the
    // rejection (with the rebase-conflict `criteria_checked` that caused
    // it), so no warning duplicates that event. A `done` names the commit
    // the tree stands at once the work is in it, which is what lets another
    // run tell whether its own tree has that work.
    let changed = match outcome {
        IntegrationOutcome::Integrated { commit } => {
            TaskStatusChangedPayload::done(task.id.clone(), *last_check_seq, commit)
        }
        IntegrationOutcome::Rejected => {
            TaskStatusChangedPayload::to(task.id.clone(), TaskStatus::Pending, *last_check_seq)
        }
    };
    ctx.emit(
        Some(&node.id),
        EventPayload::Tasks(TaskEvent::StatusChanged(changed)),
    )
    .await?;
    Ok(())
}

/// What a settled member owes a person once the batch is on the log: its
/// escalated request, when it needs a decision, and its departure from the
/// plan, when its cause is one. Departures are put to a person once the batch
/// is on the log.
fn owed_of(
    task: &Task,
    escalated: Option<(u32, ScopeExpansionOutcome)>,
    needs_human_decision: bool,
    blocked: Option<TaskCause>,
) -> Settled {
    // A task a departure blocks is reopened by its answer, never by
    // the answer to a scope request its done work also made.
    let was_blocked = blocked
        .as_ref()
        .is_some_and(|cause| !matches!(cause, TaskCause::DeviationOwed { .. }));
    let departure = match blocked {
        Some(TaskCause::DeviationOwed { deviations }) => Some(PendingDeparture {
            task_id: task.id.clone(),
            deviations,
        }),
        _ => None,
    };
    let escalation = match escalated {
        Some((attempt_no, outcome)) if needs_human_decision => Some(PendingEscalation {
            task_id: task.id.clone(),
            attempt_no,
            outcome,
            was_blocked,
        }),
        _ => None,
    };
    Settled::Done {
        escalation,
        departure,
    }
}
