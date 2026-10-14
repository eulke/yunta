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
mod iteration;
mod reopen;
mod respecify;
mod unstopped;

use yunta_core::events::{Failure, TokenUsage};
use yunta_core::{Node, NodeKind, PromptSource, ScopeExpansionMode, TasksFile};

use crate::replay::RunState;

use super::node_close::fail;
use super::node_exec::{render_or_fail, NodeEnd};
use super::prompt_exec::prompt_text;
use super::runner_resolve::{report_declarative_network, resolve_node_runner};
use super::step::Step;
use super::{RunCtx, RunError};
use batch::select_batch;
use iteration::{batch_round, depart_batch, idle_round, past_cap};

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
            match idle_round(ctx, node, &mut prep, &mut state, &view).await? {
                Some(end) => return Ok(end),
                None => continue,
            }
        }
        if let Some(end) = past_cap(ctx, node, &prep, &mut state).await? {
            return Ok(end);
        }
        let departures =
            match batch_round(ctx, node, &prep, &mut state, &view, &batch, cancel).await? {
                Step::Value(departures) => departures,
                Step::Ended(end) => return Ok(end),
            };
        if let Some(end) = depart_batch(ctx, node, &mut prep, state.tokens, departures).await? {
            return Ok(end);
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
