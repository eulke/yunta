//! Integrating a batch's finished tasks into the loop's worktree: one
//! task at a time, its criteria re-run after the rebase, and the result
//! committed or recorded as a conflict.

use std::path::PathBuf;

use yunta_core::events::{
    CriteriaCheckedPayload, CriterionResult, CriterionType, EventPayload, Phase,
    ScopeCheckedPayload, TaskStatus, TaskStatusChangedPayload,
};
use yunta_core::{CommitSha, Node, ScopeGlob, Seq, Task};

use crate::scope::audit;
use crate::task_cycle::{post_check, CriterionRun, Memo, TaskCycleReport, TaskOutcome};
use crate::worktree::{commit_work, land, rebase_onto, Rebase, Unit};

use super::escalate::{emit_scope_expansion_events, PendingEscalation};
use super::{BatchIntegration, LoopState};
use crate::run::{RunCtx, RunError};
use yunta_core::events::{NodeEvent, TaskEvent};

/// Integrates one dispatched batch, serially and in declaration order
/// (never the order dispatch finished in): drains each task's attempts onto
/// the log, rebases and fast-forwards a `Done` task onto the run's current
/// tree, and marks each task's new status. A cancelled dispatch ends the
/// whole node. Any `Escalate`d scope-expansion request is collected for the
/// caller to resolve once the whole batch is on the log.
pub(super) async fn integrate_batch(
    ctx: &RunCtx<'_>,
    node: &Node,
    dispatches: Vec<Result<(&Task, Unit, TaskCycleReport), RunError>>,
    scope_expansion: Option<&yunta_core::ScopeExpansion>,
    cancel: &tokio_util::sync::CancellationToken,
    state: &mut LoopState,
    expansions_granted_this_run: &mut u32,
) -> Result<BatchIntegration, RunError> {
    let mut pending_escalations: Vec<PendingEscalation> = Vec::new();
    for dispatch in dispatches {
        let (task, unit, mut report) = dispatch?;
        let needs_human_decision = report.needs_human_decision;
        // The escalated request itself (paths, reason, criterion + its
        // pre-check exit), captured off the attempt that raised it — what the
        // escalation object is built from.
        let mut escalated: Option<(u32, crate::scope_expansion::ScopeExpansionOutcome)> = None;

        let mut last_check_seq = ctx
            .emit(
                Some(&node.id),
                EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
                    task_id: task.id.clone(),
                    phase: Phase::Pre,
                    results: to_results(&report.pre_check),
                })),
            )
            .await?;

        for attempt in report.attempts.drain(..) {
            state.tokens += attempt.tokens;
            last_check_seq = ctx
                .emit(
                    Some(&node.id),
                    EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
                        task_id: task.id.clone(),
                        phase: Phase::Post,
                        results: to_results(&attempt.post_check),
                    })),
                )
                .await?;
            ctx.emit(
                Some(&node.id),
                EventPayload::Node(NodeEvent::ScopeChecked(ScopeCheckedPayload {
                    task_id: Some(task.id.clone()),
                    diff: attempt.scope.diff,
                    violations: attempt.scope.violations,
                })),
            )
            .await?;
            // `run_task` has no ctx to record on, so it hands the
            // breach here, beside the check that found it.
            if let Some(breach) = &attempt.fence_breach {
                record_breach(ctx, node, breach).await?;
            }

            if let Some(outcome) = &attempt.scope_expansion {
                emit_scope_expansion_events(
                    ctx,
                    node,
                    &task.id,
                    attempt.attempt,
                    outcome,
                    scope_expansion,
                    expansions_granted_this_run,
                )
                .await?;
                if outcome.decision == crate::scope_expansion::Decision::Escalate {
                    escalated = Some((attempt.attempt, outcome.clone()));
                }
            }
        }

        // A cancelled dispatch ends the whole loop node without a verdict —
        // the task stays `running` in the log (orphaned), which is exactly
        // what makes a later resume re-execute it, and the node's own fate
        // follows the same root-vs-sibling rule every other kind applies.
        if matches!(report.outcome, TaskOutcome::Interrupted) {
            return Ok(BatchIntegration::Cancelled(
                crate::run::node_exec::cancelled_end(ctx, node).await?,
            ));
        }
        let blocked_cause = match report.outcome {
            TaskOutcome::Blocked { cause } => {
                ctx.emit(
                    Some(&node.id),
                    EventPayload::Tasks(TaskEvent::StatusChanged(TaskStatusChangedPayload::to(
                        task.id.clone(),
                        TaskStatus::Blocked,
                        last_check_seq,
                    ))),
                )
                .await?;
                Some(cause)
            }
            TaskOutcome::Done => {
                let outcome = integrate_task(
                    ctx,
                    node,
                    VerifiedTask {
                        task,
                        unit: &unit,
                        staged: &report.staged,
                    },
                    &ctx.memo,
                    &mut last_check_seq,
                    cancel,
                )
                .await?;
                // A rejected integration goes back to ready on the new tree —
                // back to `Pending`, so a future batch retries it
                // automatically. This is deliberately NOT `Blocked`: green in
                // isolation but broken by a sibling's integration is a timing
                // artifact of concurrency, not evidence the task itself can't
                // succeed — that verdict only comes from `run_task`'s own
                // retry exhaustion, the branch above. The `task_status_changed`
                // emitted just below is the log's record of the rejection
                // (with the rebase-conflict `criteria_checked` that caused
                // it), so no warning duplicates that event. A `done`
                // names the commit the tree stands at once the work is
                // in it, which is what lets another run tell whether its
                // own tree has that work.
                let changed = match outcome {
                    IntegrationOutcome::Integrated { commit } => {
                        TaskStatusChangedPayload::done(task.id.clone(), last_check_seq, commit)
                    }
                    IntegrationOutcome::Rejected => TaskStatusChangedPayload::to(
                        task.id.clone(),
                        TaskStatus::Pending,
                        last_check_seq,
                    ),
                };
                ctx.emit(
                    Some(&node.id),
                    EventPayload::Tasks(TaskEvent::StatusChanged(changed)),
                )
                .await?;
                // Never counted toward the loop's own "no task ready"
                // diagnostic — a `Pending` task is retriable, not stuck, so
                // there's nothing to cite a human decision for yet.
                None
            }
            // Handled by the early return above.
            TaskOutcome::Interrupted => None,
        };
        let was_blocked = blocked_cause.is_some();
        if let Some(cause) = blocked_cause {
            // An escalation-blocked task is the escalation flow's to report
            // (resolved by the caller, or the pause diagnostic) — its interim
            // Blocked never feeds the generic tail.
            if !needs_human_decision {
                state.blocked.push((task.id.clone(), cause));
            }
        }
        if needs_human_decision {
            if let Some((attempt_no, outcome)) = escalated {
                pending_escalations.push(PendingEscalation {
                    task_id: task.id.clone(),
                    attempt_no,
                    outcome,
                    was_blocked,
                });
            }
        }
    }
    Ok(BatchIntegration::Done(pending_escalations))
}

enum IntegrationOutcome {
    /// The run's tree fast-forwarded onto the task's work and now stands
    /// at this commit — where that task's work landed. A task that
    /// committed nothing lands at the integration head itself, and
    /// recording that is exact: a tree descending from it vacuously has
    /// everything the task did.
    Integrated { commit: CommitSha },
    /// The task returned to `ready`; the cause is already on the log —
    /// the rebase-conflict `criteria_checked`, or the post-integration
    /// `criteria_checked`/`scope_checked` — so the variant carries none.
    Rejected,
}

/// A task the cycle verified `Done` in its own worktree, as the
/// integration receives it: the task, the unit its work sits in, and
/// what its adapter declared it staged there.
struct VerifiedTask<'a> {
    task: &'a Task,
    unit: &'a Unit,
    staged: &'a [PathBuf],
}

/// Integrates one task verified `Done` in isolation: rebase
/// its branch onto the run's *current* integration HEAD (which may have
/// moved since this batch started, if an earlier-declared sibling
/// integrated first), re-run criteria and scope right there — green in
/// isolation is necessary, never sufficient, since a sibling's
/// integration can change the ground a task's work sits on — and only on
/// a clean re-verification does the shared worktree fast-forward onto the
/// rebased result. A rebase conflict or a post-integration criteria/scope
/// failure returns the task to `ready` on the new tree; nothing else in
/// the batch is touched.
async fn integrate_task(
    ctx: &RunCtx<'_>,
    node: &Node,
    verified: VerifiedTask<'_>,
    memo: &Memo,
    last_check_seq: &mut Seq,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<IntegrationOutcome, RunError> {
    let VerifiedTask { task, unit, staged } = verified;
    // This task's own token: a cancelled task takes its git with it.
    let supervision = ctx.supervision(cancel);
    commit_work(
        unit,
        &yunta_core::text::detailed(format!("task {}", task.id), &task.title),
        supervision,
    )
    .await?;

    let onto = match rebase_onto(unit, ctx.worktree, supervision).await? {
        Rebase::Onto(tree) => tree,
        Rebase::Conflicts(paths) => {
            // No `post_check` ran — nothing to attach the reason to but
            // the replay itself, so it is recorded the way any other
            // command outcome is: a synthetic `CriterionRun` naming the
            // git command and the paths it stopped on, through the
            // existing `CriteriaChecked` vocabulary rather than a new
            // event kind.
            *last_check_seq = ctx
                .emit(
                    Some(&node.id),
                    EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
                        task_id: task.id.clone(),
                        phase: Phase::Post,
                        results: vec![CriterionResult {
                            cmd: format!("git rebase: {}", conflict_list(&paths)),
                            exit_code: 1,
                            r#type: None,
                            reused: false,
                            duration_ms: None,
                        }],
                    })),
                )
                .await?;
            return Ok(IntegrationOutcome::Rejected);
        }
    };
    let task_worktree = unit.worktree.as_path();

    let post_runs = post_check(task, task_worktree, memo, ctx.supervision(cancel)).await?;
    *last_check_seq = ctx
        .emit(
            Some(&node.id),
            EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
                task_id: task.id.clone(),
                phase: Phase::Post,
                results: to_results(&post_runs),
            })),
        )
        .await?;
    // Re-verified on the rebased tree, so what the task answers for is
    // what it added on top of the base it landed against — which is not
    // the tree it opened on: the ground moved under it while it worked.
    //
    // Against the same scope the attempt was judged by: what the log
    // authorized for this task is as much its scope as what the tasks
    // document declared, and an audit that ignored the grants would
    // reject work a human already allowed.
    let scope = audit(
        task_worktree,
        &onto,
        &crate::run_dir::index_for(ctx.run_dir, &unit.who),
        &effective_scope(ctx, task).await?,
        staged,
        supervision,
    )
    .await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::ScopeChecked(ScopeCheckedPayload {
            task_id: Some(task.id.clone()),
            diff: scope.diff.clone(),
            violations: scope.violations.clone(),
        })),
    )
    .await?;
    let coverage = ctx.last_coverage(&node.id).await?;
    if let Some(breach) = crate::scope::fence_breach(coverage.as_ref(), &scope) {
        record_breach(ctx, node, &breach).await?;
    }

    let criteria_green = post_runs.iter().all(|r| r.exit_code == 0);
    if !criteria_green || !scope.violations.is_empty() {
        return Ok(IntegrationOutcome::Rejected);
    }

    // Integration is strictly serial — this engine is the only writer
    // of the run's tree between the replay above and here — so a merge
    // with anything to reconcile is that invariant breaking, which
    // `land` reports as the engine error it is.
    let commit = land(unit, ctx.worktree, supervision).await?;
    Ok(IntegrationOutcome::Integrated { commit })
}

/// What this task may touch: what it declared, plus every path a
/// `scope_expansion_granted` on the log authorized for it. Derived from
/// the log rather than carried from the attempt, so a re-verification
/// after a crash grants exactly what the attempt was granted.
async fn effective_scope(ctx: &RunCtx<'_>, task: &Task) -> Result<Vec<ScopeGlob>, RunError> {
    let view = ctx.run_view().await?;
    Ok(task
        .scope
        .iter()
        .cloned()
        .chain(view.state.grants.paths_for(&task.id).iter().cloned())
        .collect())
}

/// The paths a replay stopped on, as the criterion line names them.
fn conflict_list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn to_results(runs: &[CriterionRun]) -> Vec<CriterionResult> {
    runs.iter()
        .map(|run| CriterionResult {
            cmd: run.cmd.clone(),
            exit_code: run.exit_code,
            r#type: run.is_guard.then_some(CriterionType::Guard),
            reused: run.reused,
            duration_ms: run.duration_ms,
        })
        .collect()
}

/// A write the adapter said its fence would have stopped, filed against
/// the adapter that said so.
async fn record_breach(
    ctx: &RunCtx<'_>,
    node: &Node,
    breach: &crate::scope::Breach,
) -> Result<(), RunError> {
    if let Some(adapter) = ctx.resolved_adapter(&node.id).await? {
        ctx.record_breach(&node.id, &adapter, breach).await?;
    }
    Ok(())
}
