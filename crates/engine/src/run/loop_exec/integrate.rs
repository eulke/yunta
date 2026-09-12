//! Integrating a batch's finished tasks into the loop's worktree: one
//! task at a time, its criteria re-run after the rebase, and the result
//! committed or recorded as a conflict.

use std::path::{Path, PathBuf};

use yunta_core::events::{
    CriteriaCheckedPayload, CriterionResult, CriterionType, EventPayload, Phase,
    ScopeCheckedPayload, TaskStatus, TaskStatusChangedPayload,
};
use yunta_core::{CommitSha, InvalidId, Node, Seq, Task};

use crate::scope::scope_check;
use crate::task_cycle::{post_check, CriterionRun, Memo, TaskCycleReport, TaskOutcome};

use super::escalate::{emit_scope_expansion_events, PendingEscalation};
use super::{BatchIntegration, LoopState};
use crate::run::{RunCtx, RunError};

/// Integrates one dispatched batch, serially and in declaration order
/// (never the order dispatch finished in): drains each task's attempts onto
/// the log, rebases and fast-forwards a `Done` task onto the run's current
/// tree, and marks each task's new status. A cancelled dispatch ends the
/// whole node. Any `Escalate`d scope-expansion request is collected for the
/// caller to resolve once the whole batch is on the log.
pub(super) async fn integrate_batch(
    ctx: &RunCtx<'_>,
    node: &Node,
    dispatches: Vec<Result<(&Task, PathBuf, TaskCycleReport), RunError>>,
    scope_expansion: Option<&yunta_core::ScopeExpansion>,
    cancel: &tokio_util::sync::CancellationToken,
    state: &mut LoopState,
    expansions_granted_this_run: &mut u32,
) -> Result<BatchIntegration, RunError> {
    let mut pending_escalations: Vec<PendingEscalation> = Vec::new();
    for dispatch in dispatches {
        let (task, task_worktree, mut report) = dispatch?;
        let needs_human_decision = report.needs_human_decision;
        // The escalated request itself (paths, reason, criterion + its
        // pre-check exit), captured off the attempt that raised it — what the
        // escalation object is built from.
        let mut escalated: Option<(u32, crate::scope_expansion::ScopeExpansionOutcome)> = None;

        let mut last_check_seq = ctx
            .emit(
                Some(&node.id),
                EventPayload::CriteriaChecked(CriteriaCheckedPayload {
                    task_id: task.id.clone(),
                    phase: Phase::Pre,
                    results: to_results(&report.pre_check),
                }),
            )
            .await?;

        for attempt in report.attempts.drain(..) {
            state.tokens += attempt.tokens;
            last_check_seq = ctx
                .emit(
                    Some(&node.id),
                    EventPayload::CriteriaChecked(CriteriaCheckedPayload {
                        task_id: task.id.clone(),
                        phase: Phase::Post,
                        results: to_results(&attempt.post_check),
                    }),
                )
                .await?;
            ctx.emit(
                Some(&node.id),
                EventPayload::ScopeChecked(ScopeCheckedPayload {
                    task_id: Some(task.id.clone()),
                    diff: attempt.scope.diff,
                    violations: attempt.scope.violations,
                }),
            )
            .await?;

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
        let blocked_reason = match report.outcome {
            TaskOutcome::Blocked { reason } => {
                ctx.emit(
                    Some(&node.id),
                    EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                        task_id: task.id.clone(),
                        new_status: TaskStatus::Blocked,
                        caused_by: last_check_seq,
                    }),
                )
                .await?;
                Some(reason)
            }
            TaskOutcome::Done => {
                let outcome = integrate_task(
                    ctx,
                    node,
                    VerifiedTask {
                        task,
                        worktree: &task_worktree,
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
                // it), so no warning duplicates that event.
                let new_status = match &outcome {
                    IntegrationOutcome::Integrated => TaskStatus::Done,
                    IntegrationOutcome::Rejected => TaskStatus::Pending,
                };
                ctx.emit(
                    Some(&node.id),
                    EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                        task_id: task.id.clone(),
                        new_status,
                        caused_by: last_check_seq,
                    }),
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
        let was_blocked = blocked_reason.is_some();
        if let Some(reason) = blocked_reason {
            // An escalation-blocked task is the escalation flow's to report
            // (resolved by the caller, or the pause diagnostic) — its interim
            // Blocked never feeds the generic tail.
            if !needs_human_decision {
                state
                    .blocked_reasons
                    .push(format!("task `{}` blocked: {reason}", task.id));
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
    Integrated,
    /// The task returned to `ready`; the cause is already on the log —
    /// the rebase-conflict `criteria_checked`, or the post-integration
    /// `criteria_checked`/`scope_checked` — so the variant carries none.
    Rejected,
}

/// A task the cycle verified `Done` in its own worktree, as the
/// integration receives it: the task, where its work sits, and what
/// its adapter declared it staged there.
struct VerifiedTask<'a> {
    task: &'a Task,
    worktree: &'a Path,
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
    let VerifiedTask {
        task,
        worktree: task_worktree,
        staged,
    } = verified;
    commit_task_work(task_worktree, task).await?;

    let integration_head = head_commit(ctx.worktree).await?;
    if !run_git_ok(task_worktree, &["rebase", integration_head.as_str()]).await? {
        let _ = crate::git::success(task_worktree, &["rebase", "--abort"]).await;
        // No `post_check` ran — nothing to attach the reason to but the
        // rebase itself, so it's recorded the same way any other command
        // outcome is: a synthetic `CriterionRun` naming the git command
        // and its (failing) exit code, through the existing
        // `CriteriaChecked` vocabulary rather than a new event kind.
        *last_check_seq = ctx
            .emit(
                Some(&node.id),
                EventPayload::CriteriaChecked(CriteriaCheckedPayload {
                    task_id: task.id.clone(),
                    phase: Phase::Post,
                    results: vec![CriterionResult {
                        cmd: format!("git rebase {integration_head}"),
                        exit_code: 1,
                        r#type: None,
                        reused: false,
                        duration_ms: None,
                    }],
                }),
            )
            .await?;
        return Ok(IntegrationOutcome::Rejected);
    }

    let post_runs = post_check(task, task_worktree, memo, ctx.supervision(cancel)).await?;
    *last_check_seq = ctx
        .emit(
            Some(&node.id),
            EventPayload::CriteriaChecked(CriteriaCheckedPayload {
                task_id: task.id.clone(),
                phase: Phase::Post,
                results: to_results(&post_runs),
            }),
        )
        .await?;
    let scope = scope_check(task_worktree, &task.scope, staged).await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::ScopeChecked(ScopeCheckedPayload {
            task_id: Some(task.id.clone()),
            diff: scope.diff.clone(),
            violations: scope.violations.clone(),
        }),
    )
    .await?;

    let criteria_green = post_runs.iter().all(|r| r.exit_code == 0);
    if !criteria_green || !scope.violations.is_empty() {
        return Ok(IntegrationOutcome::Rejected);
    }

    let task_head = head_commit(task_worktree).await?;
    if !run_git_ok(ctx.worktree, &["merge", "--ff-only", task_head.as_str()]).await? {
        // Integration is strictly serial (the engine itself, not
        // an external actor, is the only writer to `ctx.worktree` between
        // reading `integration_head` above and this merge) — a non-fast-
        // forward here means that invariant broke, not a legitimate task
        // outcome, so it surfaces as an engine error rather than a
        // `ready` retry.
        return Err(RunError::Git {
            context: format!("fast-forward integration of task `{}`", task.id),
            detail: "expected a clean fast-forward after rebase but git refused it".to_string(),
        });
    }
    Ok(IntegrationOutcome::Integrated)
}

/// Commits a done task's work in `cwd` — the task's own isolated worktree
/// during integration, or the run's shared worktree when
/// `concurrency` never applies. A task that changed nothing (its criteria
/// were satisfied by side effects that left no diff) simply produces no
/// commit — never an error.
async fn commit_task_work(cwd: &Path, task: &Task) -> Result<(), RunError> {
    let git_error = |e: crate::git::GitError, action: &str| RunError::Git {
        context: format!("{action} task `{}` work", task.id),
        detail: e.detail(),
    };
    crate::git::output(cwd, &["add", "-A"])
        .await
        .map_err(|e| git_error(e, "stage"))?;

    // `diff --cached --quiet` exits 0 with nothing staged, 1 with staged
    // changes — both are answers, not failures.
    if crate::git::success(cwd, &["diff", "--cached", "--quiet"])
        .await
        .map_err(|e| git_error(e, "inspect staged work for"))?
    {
        return Ok(()); // nothing staged — nothing to commit
    }

    crate::git::output(
        cwd,
        &[
            "commit",
            "-q",
            "-m",
            &format!("task {}: {}", task.id, task.title),
        ],
    )
    .await
    .map_err(|e| git_error(e, "commit"))?;
    Ok(())
}

async fn run_git_ok(cwd: &Path, args: &[&str]) -> Result<bool, RunError> {
    crate::git::success(cwd, args)
        .await
        .map_err(|e| RunError::Git {
            context: format!("run git {}", e.args),
            detail: e.detail(),
        })
}

pub(super) async fn head_commit(repo: &Path) -> Result<CommitSha, RunError> {
    let context = "read the integration HEAD commit";
    crate::git::output(repo, &["rev-parse", "HEAD"])
        .await
        .map_err(|e| RunError::Git {
            context: context.to_string(),
            detail: e.detail(),
        })?
        .trim()
        .parse()
        .map_err(|e: InvalidId| RunError::Git {
            context: context.to_string(),
            detail: e.to_string(),
        })
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
