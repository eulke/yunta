//! One iteration of a loop. A round that forms no batch either ends the node
//! or waits on what it owes; past the iteration cap a person decides whether
//! the loop goes on; a batch is dispatched and integrated, and the departures
//! it leaves are put to a person.

use yunta_core::events::{
    ChildEvent, EngineFact, EventPayload, Failure, LoopIterationPayload, NodeOutcome, TaskCause,
    TaskStatus, TaskStop, TokenUsage, UnpassableTask,
};
use yunta_core::{Node, Task, TaskId};

use crate::replay::RunView;
use crate::run::node_close::{close_node, fail_with, fail_with_tokens, Close};
use crate::run::node_exec::NodeEnd;
use crate::run::step::Step;
use crate::run::{RunCtx, RunError};
use crate::scope_expansion::GrantWindow;
use crate::worktree::head_commit;

use super::depart::{self, resolve_departures, PendingDeparture};
use super::dispatch::{dispatch_task_in_isolation, BatchDispatchEnv};
use super::escalate::{self, resolve_escalations};
use super::integrate::{integrate_batch, BatchIntegration};
use super::{LoopPrep, LoopState};

/// What an iteration that forms no batch does: it records the iteration, puts
/// each departure no person has answered to them again, and otherwise ends the
/// node or goes on. `None` when the loop goes on to its next iteration.
pub(super) async fn idle_round(
    ctx: &RunCtx<'_>,
    node: &Node,
    prep: &mut LoopPrep<'_>,
    state: &mut LoopState,
    view: &RunView,
) -> Result<Option<NodeEnd>, RunError> {
    let not_done: Vec<TaskId> = prep
        .tasks
        .tasks
        .iter()
        .filter(|task| view.state.tasks.status(&task.id) != Some(TaskStatus::Done))
        .map(|task| task.id.clone())
        .collect();
    let all_done = not_done.is_empty();
    ctx.emit(
        Some(&node.id),
        EventPayload::Children(ChildEvent::LoopIteration(LoopIterationPayload {
            iteration: state.iteration,
            until_result: all_done,
        })),
    )
    .await?;
    let stopped = view.state.tasks.stops();
    // A departure no person has answered is put again on each invocation
    // that finds nothing to run: a resume after an abort asks it again.
    let owed = depart::owed_by(&stopped);
    if !owed.is_empty() {
        let holders = depart::Holders {
            held: &prep.setup.held,
            spec: prep.setup.spec.as_deref(),
            writer: prep.writer.as_ref(),
        };
        if let Some(end) = resolve_departures(ctx, node, owed, &holders, state.tokens).await? {
            return Ok(Some(end));
        }
        // An accepted departure from a criterion of the plan waives
        // it: the tasks are judged again by what the log now holds.
        prep.rejudge(ctx).await?;
        return Ok(None);
    }
    end_idle(ctx, node, prep, state, view, not_done, stopped)
        .await
        .map(Some)
}

/// Ends an iteration that forms no batch and owes no departure: the node fails
/// on a criterion no task can pass, on a decision owed a person, or on what the
/// stopped tasks stopped on; it closes once every task is done, and otherwise
/// it stalls.
async fn end_idle(
    ctx: &RunCtx<'_>,
    node: &Node,
    prep: &LoopPrep<'_>,
    state: &LoopState,
    view: &RunView,
    not_done: Vec<TaskId>,
    stopped: Vec<TaskStop>,
) -> Result<NodeEnd, RunError> {
    // A criterion no task can pass is for the planner to rewrite; a
    // repeat of the declaration that failed the node before is not.
    if let Some(unpassable) = unpassable_of(&stopped) {
        let earlier = view
            .state
            .nodes
            .get(&node.id)
            .and_then(|record| record.failed_before.as_ref());
        let failure = match earlier {
            Some(earlier) if declares_again(&earlier.failure, &unpassable) => {
                Failure::unchanged(earlier.attempt, earlier.failure.clone())
            }
            _ => unpassable,
        };
        return fail_with(ctx, node, failure, state.tokens).await;
    }
    // A decision owed a person is what nothing else can stand in
    // for: the loop ends on it, done or not, and the person grants
    // or denies each request from there.
    if let Some(failure) = escalate::owed(&state.owed)? {
        return fail_with(ctx, node, failure, state.tokens).await;
    }
    if not_done.is_empty() {
        let outcome = NodeOutcome::Fact(EngineFact::TasksDone {
            count: prep.tasks.tasks.len(),
        });
        return close_node(ctx, node, Close::new(outcome, state.tokens)).await;
    }
    // What the tasks the ledger holds stopped on ends the loop: each
    // stop is kept as data, and an account's limit is the failure
    // itself, since it waits for its reset.
    if let Some(failure) = stopped_failure(stopped) {
        return fail_with(ctx, node, failure, state.tokens).await;
    }
    let failure = Failure::LoopStalled {
        not_ready: not_done,
    };
    fail_with_tokens(ctx, node, failure, state.tokens).await
}

/// Past a loop's iteration cap, a person decides once whether the loop goes on:
/// a loop a person does not continue ends at the cap. `None` while the
/// iteration may run.
pub(super) async fn past_cap(
    ctx: &RunCtx<'_>,
    node: &Node,
    prep: &LoopPrep<'_>,
    state: &mut LoopState,
) -> Result<Option<NodeEnd>, RunError> {
    if state.iteration <= prep.max_iterations || state.iterations_lifted {
        return Ok(None);
    }
    let (escalation, reason) =
        crate::run::budget::loop_overrun_escalation(&node.id, state.iteration, prep.max_iterations);
    match crate::run::budget::escalate(ctx, Some(&node.id), escalation, reason).await? {
        crate::run::budget::BudgetDecision::Continue => {
            state.iterations_lifted = true;
            Ok(None)
        }
        crate::run::budget::BudgetDecision::Pause { .. } => {
            let failure = Failure::LoopCapped {
                node: node.id.clone(),
                loop_cap: prep.max_iterations,
            };
            fail_with_tokens(ctx, node, failure, state.tokens)
                .await
                .map(Some)
        }
    }
}

/// Dispatches one batch in isolation, integrates it in declaration order, and
/// settles what it owes: each escalated request is decided on the log, and the
/// departures are returned for the caller to put to a person.
pub(super) async fn batch_round(
    ctx: &RunCtx<'_>,
    node: &Node,
    prep: &LoopPrep<'_>,
    state: &mut LoopState,
    view: &RunView,
    batch: &[&Task],
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Step<Vec<PendingDeparture>>, RunError> {
    // Every batch member's worktree branches from the same starting
    // point — one worktree per task, derived from the current base
    // commit — captured once so all N tasks work from an identical
    // snapshot.
    let base_commit = head_commit(ctx.worktree, ctx.root_supervision()).await?;
    let briefs = match briefs_for(ctx, node, batch, prep, cancel).await? {
        Step::Value(briefs) => briefs,
        Step::Ended(end) => return Ok(Step::Ended(end)),
    };
    // One grant window per batch, seeded from the grants the log holds —
    // the atomic cap every concurrent member's evaluation commits through,
    // so `max_per_run` holds exactly.
    let grants = GrantWindow::seeded(view.state.grants.granted());
    let env = BatchDispatchEnv {
        events: &view.events,
        state: &view.state,
        batch,
        base_commit: &base_commit,
        adapter: prep.adapter.as_ref(),
        scope_expansion: prep.scope_expansion,
        grants: &grants,
        cancel,
        setup: &prep.setup,
    };
    let (pending, departures) = match run_batch(ctx, node, &env, &briefs, state).await? {
        BatchIntegration::Stopped(end) => return Ok(Step::Ended(end)),
        BatchIntegration::Done {
            escalations,
            departures,
        } => (escalations, departures),
    };
    if !pending.is_empty() {
        // What was decided re-dispatches next iteration with the
        // decision on the log; what was not stays owed.
        let unresolved = resolve_escalations(ctx, node, pending, &grants).await?;
        state.owed.extend(unresolved);
    }
    Ok(Step::Value(departures))
}

/// Each batch member's brief: the loop's instruction, under the block its declared
/// `context:` resolves to for that member. A failing source ends the node, and
/// no session is spent.
async fn briefs_for(
    ctx: &RunCtx<'_>,
    node: &Node,
    batch: &[&Task],
    prep: &LoopPrep<'_>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Step<Vec<String>>, RunError> {
    // Each batch member's brief carries the loop's declared `context:` —
    // resolved per task (volatile sources fresh, stable ones from the
    // memo), audited as one `context_assembled` per task. A failing
    // source is the node's failure, before any session is spent.
    let mut briefs: Vec<String> = Vec::with_capacity(batch.len());
    for task in batch {
        match crate::run::context_resolve::resolve_for_task(
            ctx,
            node,
            (&task.id, &prep.context_memo),
            prep.adapter.capabilities().tool_naming,
            cancel,
        )
        .await?
        {
            Step::Value(Some(block)) => briefs.push(format!("{block}\n\n{}", prep.instruction)),
            Step::Value(None) => briefs.push(prep.instruction.clone()),
            Step::Ended(end) => return Ok(Step::Ended(end)),
        }
    }
    Ok(Step::Value(briefs))
}

/// Dispatches every member of the batch in its own worktree, concurrently, then
/// integrates them in declaration order once they have all settled.
async fn run_batch(
    ctx: &RunCtx<'_>,
    node: &Node,
    env: &BatchDispatchEnv<'_>,
    briefs: &[String],
    state: &mut LoopState,
) -> Result<BatchIntegration, RunError> {
    let dispatches = futures::future::join_all(
        env.batch
            .iter()
            .zip(briefs)
            .map(|(task, brief)| dispatch_task_in_isolation(ctx, node, env, task, brief)),
    )
    .await;
    integrate_batch(ctx, node, dispatches, env.cancel, state).await
}

/// Puts the departures a batch left to a person once the batch is on the log.
pub(super) async fn depart_batch(
    ctx: &RunCtx<'_>,
    node: &Node,
    prep: &mut LoopPrep<'_>,
    tokens: TokenUsage,
    departures: Vec<PendingDeparture>,
) -> Result<Option<NodeEnd>, RunError> {
    if departures.is_empty() {
        return Ok(None);
    }
    let holders = depart::Holders {
        held: &prep.setup.held,
        spec: prep.setup.spec.as_deref(),
        writer: prep.writer.as_ref(),
    };
    if let Some(end) = resolve_departures(ctx, node, departures, &holders, tokens).await? {
        return Ok(Some(end));
    }
    // An accepted departure from a criterion of the plan waives
    // it: the tasks are judged again by what the log now holds.
    prep.rejudge(ctx).await?;
    Ok(None)
}

/// The plan-unpassable failure of the tasks the ledger holds stopped on a
/// declared criterion, or `None` when none did.
fn unpassable_of(stopped: &[TaskStop]) -> Option<Failure> {
    let unpassable: Vec<UnpassableTask> = stopped
        .iter()
        .filter_map(|stop| match &stop.cause {
            TaskCause::CriterionUnpassable { declared } => Some(UnpassableTask {
                task_id: declared.task_id.clone(),
                cmd: declared.criterion.clone(),
                why: declared.why.clone(),
            }),
            _ => None,
        })
        .collect();
    (!unpassable.is_empty()).then_some(Failure::PlanUnpassable { unpassable })
}

/// What the tasks the ledger holds stopped end the loop on: the account's
/// limit when a session hit one, and otherwise the stops themselves as data.
/// `None` when no task stopped.
fn stopped_failure(stopped: Vec<TaskStop>) -> Option<Failure> {
    let limit = stopped.iter().find_map(|stop| match &stop.cause {
        TaskCause::SessionLimited(limit) => Some(limit.clone()),
        _ => None,
    });
    if let Some(limit) = limit {
        return Some(Failure::Limit { limit });
    }
    (!stopped.is_empty()).then_some(Failure::TasksStopped { stopped })
}

/// Whether `failure` names only tasks and criteria `earlier` named too. The
/// pair is the cause; the wording and the exit code of a new declaration
/// are not, so the two failures are never compared whole.
fn declares_again(earlier: &Failure, failure: &Failure) -> bool {
    let (
        Failure::PlanUnpassable { unpassable: before },
        Failure::PlanUnpassable { unpassable: now },
    ) = (earlier, failure)
    else {
        return false;
    };
    now.iter().all(|task| {
        before
            .iter()
            .any(|known| known.task_id == task.task_id && known.cmd == task.cmd)
    })
}
