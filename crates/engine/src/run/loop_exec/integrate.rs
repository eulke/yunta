//! Integrating a batch's finished tasks into the loop's worktree: one
//! task at a time, its criteria re-run after the rebase, and the result
//! committed or recorded as a conflict.

mod member;

use std::path::{Path, PathBuf};

use yunta_core::events::{
    CriteriaCheckedPayload, CriterionResult, Decider, EventPayload, NodeEvent, Phase,
    ScopeCheckedPayload,
};
use yunta_core::protected::{Closing, Protected};
use yunta_core::{CommitSha, Node, ScopeGlob, Seq, Task, TreeId};

use crate::run::node_exec::NodeEnd;
use crate::run::scope_record::{self, Recording, Subject};
use crate::task_cycle::{
    judge, scope_in_force, JudgeEnv, Judgement, Memo, Refusals, Settle, Sitting, Work,
};
use crate::worktree::{land, rebase_onto, Rebase, Unit};

use super::depart::PendingDeparture;
use super::dispatch::commit_task_work;
use super::escalate::PendingEscalation;
use super::unstopped::{end_on_unstopped, WalkStop};
use super::LoopState;
use crate::run::{RunCtx, RunError};
use member::{settle_member, Settled};

/// What integrating one batch produced: how the node ends, when the walk
/// stops it, or what is owed a person once the batch is on the log — the
/// scope requests escalated, and the departures from the plan declared.
pub(super) enum BatchIntegration {
    /// The node ends here: at a cancelled dispatch, or at a session the engine
    /// could not stop, among the dispatches from the walk's stop on.
    Stopped(NodeEnd),
    Done {
        escalations: Vec<PendingEscalation>,
        departures: Vec<PendingDeparture>,
    },
}

/// Integrates one dispatched batch, serially and in declaration order
/// (never the order dispatch finished in): records what each task's cycle
/// could not record itself — its attempts' fence breaches, scope expansion
/// requests and withdrawals; every check is already on the log, written the
/// moment it ran — rebases and fast-forwards a `Done` task onto the run's current
/// tree, and marks each task's new status. A session the engine could not
/// stop, among the dispatches from the walk's stop on, fails the node: the
/// tasks before the stop are integrated, and the tasks from the stop on keep
/// their status for a later wake. A cancelled dispatch ends the whole node.
/// Any `Escalate`d scope-expansion request is collected for the caller to
/// resolve once the whole batch is on the log.
pub(super) async fn integrate_batch(
    ctx: &RunCtx<'_>,
    node: &Node,
    dispatches: Vec<Result<super::dispatch::Dispatched<'_>, RunError>>,
    cancel: &tokio_util::sync::CancellationToken,
    state: &mut LoopState,
) -> Result<BatchIntegration, RunError> {
    let mut pending_escalations: Vec<PendingEscalation> = Vec::new();
    let mut departures: Vec<PendingDeparture> = Vec::new();
    // What each task owes a person about departures from the plan: every
    // one its sessions declared is on the log by the time they ended.
    let owed = ctx.run_view().await?.state.tasks;
    let mut batch = dispatches.into_iter();
    while let Some(dispatch) = batch.next() {
        let member = match dispatch {
            Ok(dispatched) => dispatched,
            Err(error) => {
                return end_on_unstopped(ctx, node, state, WalkStop::Failed(error), batch).await
            }
        };
        match settle_member(ctx, node, state, &owed, cancel, member).await? {
            Settled::Done {
                escalation,
                departure,
            } => {
                pending_escalations.extend(escalation);
                departures.extend(departure);
            }
            // The walk ends at this member. Its hold on the checkout is let go
            // when the binding drops, which is after the node has ended.
            Settled::Interrupted(_lease) => {
                return end_on_unstopped(ctx, node, state, WalkStop::Interrupted, batch).await;
            }
        }
    }
    Ok(BatchIntegration::Done {
        escalations: pending_escalations,
        departures,
    })
}

/// Records what the engine decided on one attempt's scope request, numbered
/// as that attempt. A refusal here is the rules' own, never a person's.
async fn record_attempt(
    ctx: &RunCtx<'_>,
    node: &Node,
    task: &Task,
    attempt_no: u32,
    outcome: &crate::scope_expansion::ScopeExpansionOutcome,
    new_request: bool,
) -> Result<(), RunError> {
    scope_record::record(
        &ctx.log(),
        &node.id,
        Recording {
            subject: Subject::Task(&task.id),
            mode: Some(super::mode_in_force(node)),
            by: Decider::Rule,
            asked: outcome.asked.as_ref(),
            decision: &outcome.decision,
            count: outcome.count,
            located: outcome.asked.is_none(),
            precheck: outcome.precheck,
            attempt: attempt_no,
            new_request,
        },
    )
    .await
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
    /// The hold on the unit's checkout, which landing lets go of.
    lease: &'a crate::worktree::Lease,
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
        unit,
        lease,
        staged,
    } = verified;
    // This task's own token: a cancelled task takes its git with it.
    let supervision = ctx.supervision(cancel);
    commit_task_work(ctx, node, task, unit, supervision).await?;

    let onto = match rebase_onto(unit, ctx.worktree, supervision).await? {
        Rebase::Onto(tree) => tree,
        Rebase::Conflicts(paths) => {
            *last_check_seq = record_conflict(ctx, node, task, &paths).await?;
            return Ok(IntegrationOutcome::Rejected);
        }
    };
    let task_worktree = unit.worktree.as_path();

    // Re-verified on the rebased tree, so what the task answers for is
    // what it added on top of the base it landed against — which is not
    // the tree it opened on: the ground moved under it while it worked.
    let judgement = judge_rebased(ctx, task, task_worktree, &onto, staged, memo, cancel).await?;
    *last_check_seq = record_judgement(ctx, node, task, &judgement).await?;
    if !judgement.closes() {
        return Ok(IntegrationOutcome::Rejected);
    }

    // Integration is strictly serial — this engine is the only writer
    // of the run's tree between the replay above and here — so a merge
    // with anything to reconcile is that invariant breaking, which
    // `land` reports as the engine error it is.
    let commit = land(unit, ctx.worktree, supervision).await?;
    // Landed, its checkout is kept for nobody, and the refs its attempts
    // kept go: the run's tree holds what this one did.
    ctx.pool.retire(unit, lease, supervision).await?;
    Ok(IntegrationOutcome::Integrated { commit })
}

/// What a task's landing is judged against: the scope in force, which the
/// grants on the log widen, and the protected set lifted by what a person
/// granted that task alone.
async fn judged_against(
    ctx: &RunCtx<'_>,
    task: &Task,
) -> Result<(Vec<ScopeGlob>, Protected), RunError> {
    let view = ctx.run_view().await?;
    let lifted = ctx
        .protected()
        .lifting(view.state.grants.person_granted_to_task(&task.id));
    let scope = scope_in_force(task, &view.state.grants.reach_for(&task.id));
    Ok((scope, lifted))
}

/// Judges a rebased task's work the way its attempt's close judges the same
/// tree: against the scope in force, with the protected set lifted by what a
/// person granted that task alone.
async fn judge_rebased(
    ctx: &RunCtx<'_>,
    task: &Task,
    checkout: &Path,
    onto: &TreeId,
    staged: &[PathBuf],
    memo: &Memo,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Judgement, RunError> {
    // Against the same scope the attempt was judged by: what the log
    // authorized for this task is as much its scope as what the tasks
    // document declared, and an audit that ignored the grants would
    // reject work a human already allowed.
    // Every test a person approved, the task's own among them: its own
    // are what it staged, which the audit leaves out of its diff.
    let (scope, lifted) = judged_against(ctx, task).await?;
    let sitting = Sitting {
        work: Work {
            checkout,
            from: onto,
            staged,
        },
        task: Some(task),
        scope: &scope,
        request: None,
        refused: Refusals::None,
        window: None,
        bounds: None,
        declared: None,
        documents: None,
        env: JudgeEnv {
            memo,
            protected: &lifted,
            closing: Closing::Every,
            supervision: ctx.supervision(cancel),
        },
    };
    Ok(judge(sitting, Settle::Commit).await?)
}

/// Records what a rebased task's judgement found, as its close records it:
/// the criteria, then the scope audit and any fence breach it shows. Returns
/// where the criteria landed on the log, which the task's status change cites.
async fn record_judgement(
    ctx: &RunCtx<'_>,
    node: &Node,
    task: &Task,
    judgement: &Judgement,
) -> Result<Seq, RunError> {
    let results = ctx.recorded(&judgement.criteria).await.map_err(|source| {
        crate::task_cycle::TaskCycleError::KeepOutput {
            task: task.id.clone(),
            source,
        }
    })?;
    let criteria_at = ctx
        .emit(
            Some(&node.id),
            EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
                task_id: task.id.clone(),
                phase: Phase::Post,
                results,
                waiting: judgement.waiting.clone(),
            })),
        )
        .await?;
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::ScopeChecked(ScopeCheckedPayload {
            task_id: Some(task.id.clone()),
            diff: judgement.scope.diff.clone(),
            violations: judgement.scope.violations.clone(),
            denied: judgement.scope.denied.clone(),
        })),
    )
    .await?;
    // Only a violation can be a breach, so the log is read for the session's
    // fence only when the check found one.
    if !judgement.scope.violations.is_empty() {
        let (coverage, confinement) = ctx.last_fence(&node.id).await?;
        if let Some(breach) =
            crate::scope::fence_breach(coverage.as_ref(), confinement.as_ref(), &judgement.scope)
        {
            ctx.record_breach(&node.id, &breach).await?;
        }
    }
    Ok(criteria_at)
}

/// Records a rebase that could not replay a task's work as the criteria check
/// that failed it. No `post_check` ran — nothing to attach the reason to but
/// the replay itself, so it is recorded the way any other command outcome is:
/// a synthetic `CriterionRun` naming the git command and the paths it stopped
/// on, through the existing `CriteriaChecked` vocabulary rather than a new
/// event kind.
async fn record_conflict(
    ctx: &RunCtx<'_>,
    node: &Node,
    task: &Task,
    paths: &[PathBuf],
) -> Result<Seq, RunError> {
    ctx.emit(
        Some(&node.id),
        EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
            task_id: task.id.clone(),
            phase: Phase::Post,
            results: vec![CriterionResult {
                cmd: format!("git rebase: {}", conflict_list(paths)),
                exit_code: 1,
                r#type: None,
                reused: false,
                duration_ms: None,
                output: None,
                tail: Vec::new(),
                tree: None,
                head: None,
                conclusion: None,
            }],
            waiting: Vec::new(),
        })),
    )
    .await
}

/// The paths a replay stopped on, as the criterion line names them.
fn conflict_list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
