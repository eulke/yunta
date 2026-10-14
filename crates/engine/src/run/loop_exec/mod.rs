//! The loop node's task cycle: each iteration forms a batch of up to
//! `concurrency` `ready` tasks (declaration order), dispatches
//! every member in its own isolated worktree concurrently, then
//! integrates them **serially, in that same declaration order** —
//! rebase onto the current tree, re-verify criteria and scope there,
//! and only then fast-forward the run's shared worktree. `concurrency:
//! 1` (the default) walks the exact same path with a batch of one —
//! there is no special case for it.

mod batch;
mod depart;
mod dispatch;
mod escalate;
mod integrate;
mod reopen;
mod respecify;
mod unstopped;

use yunta_core::events::{
    EngineFact, EventPayload, Failure, LoopIterationPayload, NodeOutcome, TaskCause, TaskStatus,
    TaskStop, TokenUsage, UnpassableTask,
};
use yunta_core::{Node, NodeKind, PromptSource, ScopeExpansionMode, TasksFile};

use crate::replay::RunState;
use crate::scope_expansion::GrantWindow;

use super::node_close::{close_node, fail, fail_with, fail_with_tokens, Close};
use super::node_exec::{render_or_fail, NodeEnd};
use super::prompt_exec::prompt_text;
use super::runner_resolve::{report_declarative_network, resolve_node_runner};
use super::step::Step;
use super::{RunCtx, RunError};
use batch::select_batch;
use depart::resolve_departures;
use dispatch::{dispatch_task_in_isolation, BatchDispatchEnv};
use escalate::resolve_escalations;
use integrate::{integrate_batch, BatchIntegration};

use crate::worktree::head_commit;
use yunta_core::events::ChildEvent;

pub(super) async fn execute_loop(
    ctx: &RunCtx<'_>,
    node: &Node,
    prompt: &PromptSource,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<NodeEnd, RunError> {
    let mut prep = match prepare_loop(ctx, node, prompt).await? {
        LoopReady::Go(prep) => *prep,
        LoopReady::Ended(end) => return Ok(end),
    };
    reopen::after_retry(ctx, node, &prep.tasks).await?;

    let mut state = LoopState {
        tokens: TokenUsage::default(),
        iteration: 0,
        iterations_lifted: false,
        owed: Vec::new(),
    };
    loop {
        state.iteration += 1;
        // Tests a person accepted are wrong are written again before
        // anything else of this loop is dispatched.
        if let Some(end) = respecify::send_back(ctx, node, state.tokens).await? {
            return Ok(end);
        }
        let view = ctx.run_view().await?;
        let batch = select_batch(&prep.tasks, &view.state, prep.concurrency);

        if batch.is_empty() {
            let not_done: Vec<_> = prep
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
                if let Some(end) =
                    resolve_departures(ctx, node, owed, &holders, state.tokens).await?
                {
                    return Ok(end);
                }
                prep.rejudge(ctx).await?;
                continue;
            }
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
            if all_done {
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
            return fail_with_tokens(ctx, node, failure, state.tokens).await;
        }

        if state.iteration > prep.max_iterations && !state.iterations_lifted {
            let (escalation, reason) = super::budget::loop_overrun_escalation(
                &node.id,
                state.iteration,
                prep.max_iterations,
            )
            .map_err(|source| RunError::Escalation {
                context: format!("loop `{}`'s escalation", node.id),
                source,
            })?;
            match super::budget::escalate(ctx, Some(&node.id), escalation, reason).await? {
                super::budget::BudgetDecision::Continue => state.iterations_lifted = true,
                super::budget::BudgetDecision::Pause { .. } => {
                    let failure = Failure::LoopCapped {
                        node: node.id.clone(),
                        loop_cap: prep.max_iterations,
                    };
                    return fail_with_tokens(ctx, node, failure, state.tokens).await;
                }
            }
        }

        // Every batch member's worktree branches from the same starting
        // point — one worktree per task, derived from the current base
        // commit — captured once so all N tasks work from an identical
        // snapshot.
        let base_commit = head_commit(ctx.worktree, ctx.root_supervision()).await?;

        // Each batch member's brief carries the loop's declared `context:` —
        // resolved per task (volatile sources fresh, stable ones from the
        // memo), audited as one `context_assembled` per task. A failing
        // source is the node's failure, before any session is spent.
        let mut briefs: Vec<String> = Vec::with_capacity(batch.len());
        for task in &batch {
            match super::context_resolve::resolve_for_task(
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
                Step::Ended(end) => return Ok(end),
            }
        }

        // One grant window per batch, seeded from the grants the log holds —
        // the atomic cap every concurrent member's evaluation commits through,
        // so `max_per_run` holds exactly.
        let grants = GrantWindow::seeded(view.state.grants.granted());
        let batch_env = BatchDispatchEnv {
            events: &view.events,
            state: &view.state,
            batch: &batch,
            base_commit: &base_commit,
            adapter: prep.adapter.as_ref(),
            scope_expansion: prep.scope_expansion,
            grants: &grants,
            cancel,
            setup: &prep.setup,
        };
        let dispatches =
            futures::future::join_all(batch.iter().zip(&briefs).map(|(task, brief)| {
                dispatch_task_in_isolation(ctx, node, &batch_env, task, brief)
            }))
            .await;

        let (pending, departures) =
            match integrate_batch(ctx, node, dispatches, cancel, &mut state).await? {
                BatchIntegration::Cancelled(end) | BatchIntegration::Ended(end) => return Ok(end),
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
        if !departures.is_empty() {
            let holders = depart::Holders {
                held: &prep.setup.held,
                spec: prep.setup.spec.as_deref(),
                writer: prep.writer.as_ref(),
            };
            if let Some(end) =
                resolve_departures(ctx, node, departures, &holders, state.tokens).await?
            {
                return Ok(end);
            }
            // An accepted departure from a criterion of the plan waives
            // it: the tasks are judged again by what the log now holds.
            prep.rejudge(ctx).await?;
        }
    }
}

/// Everything one loop invocation resolves once, before its first iteration.
struct LoopPrep<'a> {
    instruction: String,
    adapter: std::sync::Arc<dyn yunta_core::port::Adapter>,
    setup: crate::task_cycle::SessionSetup,
    /// The registered document, as its planner wrote it.
    document: TasksFile,
    /// The node that writes the run's spec again, when one of it does.
    writer: Option<yunta_core::NodeId>,
    /// The registered document, each task as the run judges it.
    tasks: TasksFile,
    concurrency: u32,
    scope_expansion: Option<&'a yunta_core::ScopeExpansion>,
    context_memo: super::context_resolve::StableContextMemo,
    max_iterations: u32,
}

/// The outcome of preparing a loop: ready to run, or already ended (a
/// missing tasks document, an unresolvable runner, a capability a blackboard needs).
enum LoopReady<'a> {
    Go(Box<LoopPrep<'a>>),
    Ended(NodeEnd),
}

/// The mutable state a loop carries across its iterations.
struct LoopState {
    tokens: TokenUsage,
    iteration: u32,
    iterations_lifted: bool,
    /// The scope requests nobody was there to decide, owed once nothing
    /// else of the loop can run.
    owed: Vec<escalate::PendingEscalation>,
}

impl LoopPrep<'_> {
    /// Judges every task again by what the log holds now — the
    /// departures a person accepted since — and gives its sessions the
    /// plan as it is judged.
    async fn rejudge(&mut self, ctx: &RunCtx<'_>) -> Result<(), RunError> {
        let view = ctx.run_view().await?;
        let held = ctx
            .memo
            .held()
            .holding(&crate::tasks::measured(&view.state.run));
        let (spec, tasks) = (self.setup.spec.as_deref(), &view.state.tasks);
        self.tasks = crate::tasks::judged_plan(&self.document, &held, spec, tasks);
        self.setup.plan = Some(std::sync::Arc::new(self.tasks.clone()));
        Ok(())
    }
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

/// Resolves everything a loop needs once, before any task runs: the rendered
/// instruction, the adapter and the session setup every task shares (skills
/// and run tools gated by the adapter's declared capabilities), the
/// registered tasks document, and the loop's own knobs. A capability a
/// blackboard group needs, a missing runner, or an unregistered tasks document ends
/// the node here, before a token is spent.
async fn prepare_loop<'a>(
    ctx: &RunCtx<'_>,
    node: &'a Node,
    prompt: &PromptSource,
) -> Result<LoopReady<'a>, RunError> {
    let instruction = match render_or_fail(ctx, node, prompt_text(ctx, node, prompt)).await? {
        Step::Value(rendered) => rendered,
        Step::Ended(end) => return Ok(LoopReady::Ended(end)),
    };
    let chosen = match resolve_node_runner(ctx, node).await? {
        Step::Value(chosen) => chosen,
        Step::Ended(end) => return Ok(LoopReady::Ended(end)),
    };
    let adapter = ctx.adapters[&chosen.adapter].clone();
    // Once for the whole loop: the network policy is the node's, not the
    // task's, so its declarative-only degradation is recorded here rather
    // than per task session.
    report_declarative_network(ctx, node, adapter.as_ref()).await?;

    let setup = match super::session_plan::resolve_setup(ctx, node, &chosen).await? {
        Ok(setup) => setup,
        Err(end) => return Ok(LoopReady::Ended(end)),
    };

    let view = ctx.run_view().await?;
    let Some(held) = crate::artifacts::latest::<TasksFile>(ctx.run_dir, &view.events).await? else {
        let failure = Failure::NoTasksDocument {
            tasks_for: node.id.clone(),
        };
        let end = fail(ctx, node, failure).await?;
        return Ok(LoopReady::Ended(end));
    };
    registered_here(&held, &view.state)?;
    // Every task this loop runs is judged the way the run judges it, once,
    // here: its pre-check, its session's own checks, its close and its
    // integration all read these criteria — its own, the commands the run
    // measured, and the tests the run's spec gives it.
    let holding = ctx
        .memo
        .held()
        .holding(&crate::tasks::measured(&view.state.run));
    let spec = crate::artifacts::latest::<yunta_core::SpecFile>(ctx.run_dir, &view.events)
        .await?
        .map(|held| std::sync::Arc::new(held.document));
    let (held_by, ledger) = (holding.as_slice(), &view.state.tasks);
    let tasks = crate::tasks::judged_plan(&held.document, held_by, spec.as_deref(), ledger);
    // Every task session reads the plan its task belongs to: the design
    // it names, and the tasks that own what it may not touch.
    let writer = respecify::writer(ctx, &view.state);
    let superseded = respecify::superseded(ctx.run_dir, &view.events, spec.as_deref()).await?;
    let setup = crate::task_cycle::SessionSetup {
        plan: Some(std::sync::Arc::new(tasks.clone())),
        spec,
        held: holding,
        superseded,
        ..setup
    };

    // Absent means the engine's own default, 1 — sequential, deliberately
    // not config-overridable: token spend multiplies with it, so it's
    // declared per-workflow, never inherited silently.
    let concurrency = match &node.kind {
        NodeKind::Loop { concurrency, .. } => concurrency.unwrap_or(1).max(1),
        _ => 1,
    };
    // Loop-scoped, never workflow/config-scoped: a request is task-specific,
    // and only a loop node's own tasks can ever write one.
    let scope_expansion = match &node.kind {
        NodeKind::Loop {
            scope_expansion, ..
        } => scope_expansion.as_ref(),
        _ => None,
    };

    Ok(LoopReady::Go(Box::new(LoopPrep {
        instruction,
        adapter,
        setup,
        document: held.document,
        writer,
        tasks,
        concurrency,
        scope_expansion,
        // Stable/run-stable context resolved once and reused across every
        // task brief this invocation builds; volatile sources re-resolve per
        // brief.
        context_memo: super::context_resolve::StableContextMemo::default(),
        // The only net under a tasks document whose state oscillates forever.
        max_iterations: ctx.manifest.config.resolved_max_loop_iterations(),
    })))
}

/// The mode a loop's scope requests are decided by: the one it declares, or
/// the default one where it declares no `scope_expansion:` block. Every
/// request the loop records or owes carries this mode.
pub(super) fn mode_in_force(node: &Node) -> ScopeExpansionMode {
    match &node.kind {
        NodeKind::Loop {
            scope_expansion, ..
        } => scope_expansion
            .as_ref()
            .map_or_else(ScopeExpansionMode::default, |scope| scope.mode),
        _ => ScopeExpansionMode::default(),
    }
}

/// Refuses a document whose tasks this run never registered, naming
/// every one of them.
///
/// Every door a tasks document enters a run by states what the run has
/// to do about it, so one the run holds without registrations describes
/// a log missing what the loop runs on. Said once, here, before the
/// first batch: the alternative is a loop that forms no batch and
/// reports that nothing is ready — a sentence about neither the
/// document nor the tasks.
fn registered_here(
    held: &crate::artifacts::Held<TasksFile>,
    state: &RunState,
) -> Result<(), RunError> {
    let missing: Vec<String> = held
        .document
        .tasks
        .iter()
        .filter(|task| !state.tasks.contains(&task.id))
        .map(|task| task.id.to_string())
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(RunError::Broken {
        diagnostic: format!(
            "this run holds a tasks document ({}) whose tasks it never registered: {}",
            held.describe,
            missing.join(", ")
        ),
    })
}
