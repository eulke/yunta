//! The loop node's task cycle: each iteration forms a batch of up to
//! `concurrency` `ready` tasks (ledger declaration order), dispatches
//! every member in its own isolated worktree concurrently, then
//! integrates them **serially, in that same declaration order** —
//! rebase onto the current tree, re-verify criteria and scope there,
//! and only then fast-forward the run's shared worktree. `concurrency:
//! 1` (the default) walks the exact same path with a batch of one —
//! there is no special case for it.

mod dispatch;
mod escalate;
mod integrate;

use yunta_core::events::{EventPayload, LoopIterationPayload, StoredEvent, TaskStatus, TokenUsage};
use yunta_core::{Ledger, Node, NodeKind, PromptSource, Task};

use crate::replay::RunState;

use super::node_close::{close_node, fail, fail_with_tokens, Close};
use super::node_exec::{render_or_fail, NodeEnd};
use super::prompt_exec::prompt_text;
use super::runner_resolve::{report_declarative_network, resolve_node_runner};
use super::step::Step;
use super::{RunCtx, RunError};
use dispatch::{dispatch_task_in_isolation, BatchDispatchEnv};
use escalate::{resolve_escalations, PendingEscalation};
use integrate::{head_commit, integrate_batch};

pub(super) async fn execute_loop(
    ctx: &RunCtx<'_>,
    node: &Node,
    prompt: &PromptSource,
    attempt: u32,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<NodeEnd, RunError> {
    let prep = match prepare_loop(ctx, node, prompt).await? {
        LoopReady::Go(prep) => *prep,
        LoopReady::Ended(end) => return Ok(end),
    };

    let mut state = LoopState {
        tokens: TokenUsage::default(),
        iteration: 0,
        iterations_lifted: false,
        blocked_reasons: Vec::new(),
    };
    loop {
        state.iteration += 1;
        let view = ctx.run_view().await?;
        let batch = select_batch(&prep.ledger, &view.state, prep.concurrency);

        if batch.is_empty() {
            let all_done = prep
                .ledger
                .tasks
                .iter()
                .all(|task| view.state.tasks.get(&task.id) == Some(&TaskStatus::Done));
            ctx.emit(
                Some(&node.id),
                EventPayload::LoopIteration(LoopIterationPayload {
                    iteration: state.iteration,
                    until_result: all_done,
                }),
            )
            .await?;
            if all_done {
                return close_node(
                    ctx,
                    node,
                    Close::new(
                        format!("{} task(s) done", prep.ledger.tasks.len()),
                        state.tokens,
                        attempt,
                        cancel,
                    ),
                )
                .await;
            }
            let mut diagnostic = "no task is ready and not all are done — blocked or failed \
                                  tasks need a decision"
                .to_string();
            for reason in &state.blocked_reasons {
                diagnostic.push_str("; ");
                diagnostic.push_str(reason);
            }
            return fail_with_tokens(ctx, node, diagnostic, false, state.tokens).await;
        }

        if state.iteration > prep.max_iterations && !state.iterations_lifted {
            let (escalation, reason) = super::budget::loop_overrun_escalation(
                &node.id,
                state.iteration,
                prep.max_iterations,
            );
            match super::budget::escalate(ctx, Some(&node.id), escalation, reason).await? {
                super::budget::BudgetDecision::Continue => state.iterations_lifted = true,
                super::budget::BudgetDecision::Pause { reason } => {
                    return fail_with_tokens(ctx, node, reason, false, state.tokens).await;
                }
            }
        }

        // Every batch member's worktree branches from the same starting
        // point — one worktree per task, derived from the current base
        // commit — captured once so all N tasks work from an identical
        // snapshot.
        let base_commit = head_commit(ctx.worktree).await?;

        // Each batch member's brief carries the loop's declared `context:` —
        // resolved per task (volatile sources fresh, stable ones from the
        // memo), audited as one `context_assembled` per task. A failing
        // source is the node's failure, before any session is spent.
        let mut briefs: Vec<String> = Vec::with_capacity(batch.len());
        for task in &batch {
            match super::context_resolve::resolve_for_task(
                ctx,
                node,
                &task.id,
                &prep.context_memo,
                cancel,
            )
            .await?
            {
                Step::Value(Some(block)) => briefs.push(format!("{block}\n\n{}", prep.instruction)),
                Step::Value(None) => briefs.push(prep.instruction.clone()),
                Step::Ended(end) => return Ok(end),
            }
        }

        // One grant ledger per batch, seeded from the log — the atomic cap
        // window every concurrent member's evaluation commits through, so
        // `max_per_run` holds exactly.
        let grants = crate::scope_expansion::GrantLedger::new(granted_count(&view.events));
        let batch_env = BatchDispatchEnv {
            events: &view.events,
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

        // Cumulative grants this run, kept live across the integration and
        // escalation phases below so each emitted `ScopeExpansionGranted`
        // carries an accurate `count_this_run` — the cap *decision* already
        // happened atomically in the batch's `GrantLedger`; this count only
        // feeds the event payload.
        let mut expansions_granted_this_run = granted_count(&view.events);
        let pending = match integrate_batch(
            ctx,
            node,
            dispatches,
            prep.scope_expansion,
            cancel,
            &mut state,
            &mut expansions_granted_this_run,
        )
        .await?
        {
            BatchIntegration::Cancelled(end) => return Ok(end),
            BatchIntegration::Done(pending) => pending,
        };

        if !pending.is_empty() {
            if let Some(end) = resolve_escalations(
                ctx,
                node,
                pending,
                prep.scope_expansion,
                &mut expansions_granted_this_run,
                state.tokens,
            )
            .await?
            {
                return Ok(end);
            }
            // Everything resolved — the next iteration re-dispatches the (now
            // Pending again) tasks with the decisions on the log.
        }
    }
}

/// Everything one loop invocation resolves once, before its first iteration.
struct LoopPrep<'a> {
    instruction: String,
    adapter: std::sync::Arc<dyn yunta_adapters::Adapter>,
    setup: crate::task_cycle::SessionSetup,
    ledger: Ledger,
    concurrency: u32,
    scope_expansion: Option<&'a yunta_core::ScopeExpansion>,
    context_memo: super::context_resolve::StableContextMemo,
    max_iterations: u32,
}

/// The outcome of preparing a loop: ready to run, or already ended (a
/// missing ledger, an unresolvable runner, a capability a blackboard needs).
enum LoopReady<'a> {
    Go(Box<LoopPrep<'a>>),
    Ended(NodeEnd),
}

/// The mutable state a loop carries across its iterations.
struct LoopState {
    tokens: TokenUsage,
    iteration: u32,
    iterations_lifted: bool,
    /// Blocked reasons gathered this invocation, so the loop's own failure
    /// can cite them. A resume starts empty — the log carries each task's
    /// status, and the empty-batch tail still names which tasks are blocked.
    blocked_reasons: Vec<String>,
}

/// What integrating one batch produced: a cancellation that ends the node,
/// or the escalations owed a human once the batch is on the log.
enum BatchIntegration {
    Cancelled(NodeEnd),
    Done(Vec<PendingEscalation>),
}

/// Resolves everything a loop needs once, before any task runs: the rendered
/// instruction, the adapter and the session setup every task shares (skills
/// and run tools gated by the adapter's declared capabilities), the
/// registered task ledger, and the loop's own knobs. A capability a
/// blackboard group needs, a missing runner, or an unregistered ledger ends
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
    // Once for the whole loop, like skills below: the network policy is the
    // node's, not the task's, so its declarative-only degradation is recorded
    // here rather than per task session.
    report_declarative_network(ctx, node, adapter.as_ref(), &chosen.adapter).await?;

    // One resolution for the whole loop — every task session mounts the same
    // skills, and a missing name fails the node before any token is spent.
    let skills = match crate::skills::resolve_skills(
        &ctx.manifest.config,
        &ctx.manifest.workflow,
        node,
        ctx.worktree,
    ) {
        Ok(skills) => skills,
        Err(error) => {
            return Ok(LoopReady::Ended(
                fail(ctx, node, error.to_string(), false).await?,
            ))
        }
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
                                 mechanism; task sessions run without them"
                    .to_string(),
            }),
        )
        .await?;
        Vec::new()
    } else {
        skills
    };
    // Same gating as a prompt session — the capability decides, and a
    // blackboard-group loop on a capability-less adapter fails rather than
    // silently dropping its declared coordination.
    let run_tools = if adapter
        .capabilities()
        .declares(yunta_core::Capability::RunTools)
    {
        Some(crate::run_tools::RunToolsAccess {
            host: ctx.run_tools_host.clone(),
            node: node.id.clone(),
            // A task session writes into the loop node's own declared
            // artifacts, so it gets to check them: the file it writes is
            // the file that node closes on.
            declared: super::node_exec::declared_artifacts(ctx, node),
        })
    } else {
        if ctx.run_tools_host.is_blackboard_member(&node.id) {
            let end = fail(
                ctx,
                node,
                format!(
                    "node `{}` is in a `coordination: blackboard` group but adapter `{}` \
                     declares no `run_tools` capability — the blackboard cannot be mounted",
                    node.id, chosen.adapter
                ),
                false,
            )
            .await?;
            return Ok(LoopReady::Ended(end));
        }
        None
    };
    let setup = crate::task_cycle::SessionSetup {
        skills,
        adapter_settings: ctx.adapter_settings(&chosen.adapter),
        env: crate::task_cycle::SessionSetup::secrets_env(&ctx.manifest.config),
        run_tools,
        run_dir: ctx.run_dir.to_path_buf(),
    };

    let Some(ledger) = load_registered_ledger(ctx)? else {
        let end = fail(
            ctx,
            node,
            "no task ledger has been registered before this loop — a previous node must \
             produce an artifact with `kind: task-ledger`"
                .to_string(),
            false,
        )
        .await?;
        return Ok(LoopReady::Ended(end));
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
        ledger,
        concurrency,
        scope_expansion,
        // Stable/run-stable context resolved once and reused across every
        // task brief this invocation builds; volatile sources re-resolve per
        // brief.
        context_memo: super::context_resolve::StableContextMemo::default(),
        // The only net under a ledger whose state oscillates forever.
        max_iterations: ctx.manifest.config.resolved_max_loop_iterations(),
    })))
}

/// Up to `concurrency` tasks this iteration may work on, in ledger
/// declaration order: a task whose dependencies are all `Done` and is
/// itself still `Pending`, or an orphaned `Running` task with no
/// terminal event after it (a crash mid-batch — orphaned tasks always
/// get re-run on resume). Scope disjointness between independent tasks
/// is **not** re-checked here: `ledger::register` already refuses two
/// tasks without a `depends_on` edge declaring overlapping scope, so
/// any two tasks that can both be `ready` at once are disjoint by
/// construction.
fn select_batch<'a>(ledger: &'a Ledger, state: &RunState, concurrency: u32) -> Vec<&'a Task> {
    ledger
        .tasks
        .iter()
        .filter(|task| match state.tasks.get(&task.id) {
            Some(TaskStatus::Pending) => task
                .depends_on
                .iter()
                .all(|dep| state.tasks.get(dep) == Some(&TaskStatus::Done)),
            Some(TaskStatus::Running) => true,
            _ => false,
        })
        .take(concurrency as usize)
        .collect()
}

/// How many `scope_expansion_granted` events the run's whole log already
/// has — `max_per_run` is run-scoped, never per-task. Read once per
/// batch to seed that batch's `GrantLedger`: grants from prior
/// batches are already events by then (integration is serial and
/// completes before the next batch dispatches), and grants *within* the
/// batch go through the ledger's atomic window — so the cap holds
/// exactly, with dispatch itself still fully concurrent.
fn granted_count(events: &[StoredEvent]) -> u32 {
    events
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::ScopeExpansionGranted(_))
            )
        })
        .count() as u32
}

/// Finds the task ledger the run registered: the `kind: task-ledger`
/// artifact of a node that produced it earlier, re-read from the run's
/// frozen `artifacts/` — artifacts are immutable once written.
fn load_registered_ledger(ctx: &RunCtx<'_>) -> Result<Option<Ledger>, RunError> {
    for node in &ctx.manifest.workflow.nodes {
        let Some(artifacts) = &node.artifacts else {
            continue;
        };
        for spec in &artifacts.produces {
            let yunta_core::ArtifactSpec::Typed { name, kind } = spec else {
                continue;
            };
            if !matches!(kind, yunta_core::ArtifactKind::TaskLedger) {
                continue;
            }
            let path = ctx.run_dir.join("artifacts").join(name);
            if !path.exists() {
                continue;
            }
            let bytes = std::fs::read(&path).map_err(|source| RunError::Io {
                context: format!("read task ledger `{}`", path.display()),
                source,
            })?;
            // The same door `close_artifacts` reads a ledger through, so
            // a file that stops being readable between the node that
            // wrote it and the loop that consumes it is reported as the
            // document it is, with every problem named.
            let ledger = yunta_core::shape::read::<Ledger>(&bytes, path.display().to_string())?;
            return Ok(Some(ledger));
        }
    }
    Ok(None)
}
