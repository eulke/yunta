//! The run execution driver: the scheduler loop that replays the log,
//! asks [`schedule::decide`] what is next, and runs it until the answer
//! is terminal — plus the pause paths every stop funnels through and the
//! node lookup the step handlers share.
//!
//! Crash, restart and Ctrl-C are the same case: whatever the log says
//! happened, happened; everything else re-runs. A resume re-enters
//! [`execute_run_at_depth`] and reads what remains off the log, never from
//! in-process state.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{
    EventDraft, EventPayload, FindingSeverity, PauseReason, RunPausedPayload,
};
use yunta_core::{Clock, ModeName, NodeId, Pid, RunId};
use yunta_storage::AsyncStorage;

use crate::task_cycle::Memo;

use super::schedule::{self, Decision};
use super::{baseline, gate_exec, steps, RunCtx, RunEnv, RunError, RunReport, RunTerminal};
use yunta_core::events::RunEvent;
use yunta_core::Location;

/// Records the `run_paused` a post-crash `yunta cancel` writes when it
/// finds the engine already dead. The CLI never builds an `EventDraft`
/// itself: event construction and its clock stamp live here, so every
/// event on the log is emitted by the engine through one injected clock.
/// The run-level `run_paused` event — built in one place so every path
/// that stops a run (the scheduler's [`record_pause`], a crash's
/// [`record_pause_after_crash`]) writes the same event, and its line is
/// the reason's own `Display`.
fn run_paused(reason: &PauseReason) -> EventPayload {
    EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(reason)))
}

pub async fn record_pause_after_crash(
    storage: &AsyncStorage,
    run_id: &RunId,
    reason: &PauseReason,
    clock: &dyn Clock,
) -> Result<(), RunError> {
    storage
        .append(
            EventDraft {
                run_id: run_id.clone(),
                node_id: None,
                payload: run_paused(reason),
            },
            clock.now(),
        )
        .await?;
    Ok(())
}

/// Records the run's pause, exports the forensic `events.jsonl`, and returns
/// the paused report — the one place a run stops for a human to resume,
/// whatever asked for it (a cancellation, an exhausted budget, an
/// unanswered gate).
pub(super) async fn pause(ctx: &RunCtx<'_>, reason: PauseReason) -> Result<RunReport, RunError> {
    record_pause(ctx, &reason).await?;
    Ok(RunReport {
        terminal: RunTerminal::Paused {
            reason: reason.to_string(),
        },
        state: ctx.run_view().await?.state,
    })
}

/// Emits the run-level `run_paused` event and exports the forensic log — the
/// one place the pause event is written, shared by the scheduler's own pause
/// and the gate flow's.
async fn record_pause(ctx: &RunCtx<'_>, reason: &PauseReason) -> Result<(), RunError> {
    ctx.emit(None, run_paused(reason)).await?;
    ctx.export_events_jsonl().await
}

/// Everything the scheduler loop runs on, once the run has woken: its
/// built context, the loop's own handle on the root cancellation, the
/// frozen mode, and the policy every decision reads. `Finished`
/// short-circuits a run whose log already ends.
/// Both sides boxed: a run starts once, so the allocation is nothing
/// beside carrying either side's weight in every move of the other.
enum Startup<'a> {
    Finished(Box<RunReport>),
    Ready(Box<Ready<'a>>),
}

/// Everything the loop needs, for a run that has one to do.
struct Ready<'a> {
    ctx: RunCtx<'a>,
    root_cancel: CancellationToken,
    mode_name: ModeName,
    policy: schedule::Policy,
}

/// [`execute_run`] with an explicit composition depth:
/// `workflow_exec` re-enters here for each child run, one level deeper —
/// the recursion "a parent's resume recursively resumes orphaned
/// children" is made of.
#[tracing::instrument(skip_all, fields(run_id = %env.run_id, depth))]
pub(crate) async fn execute_run_at_depth(
    env: RunEnv<'_>,
    depth: u32,
) -> Result<RunReport, RunError> {
    let ready = match start(env, depth).await? {
        Startup::Finished(report) => return Ok(*report),
        Startup::Ready(ready) => *ready,
    };
    // The run's clock is read every second for as long as the loop
    // drives it, so a host that slept is on the log even while nothing
    // else is being written. The loop ending ends the watch. The loop is
    // boxed: a child run re-enters this function, and carrying both
    // futures inline would grow every level of that recursion.
    let log = ready.ctx.log();
    tokio::select! {
        biased;
        report = Box::pin(drive(&ready)) => report,
        stopped = ready.ctx.awake.watch(&log) => match stopped {
            Ok(never) => match never {},
            Err(error) => Err(error.into()),
        },
    }
}

/// The scheduler loop: replays the log, asks what is next, and runs it
/// until the answer is terminal.
///
/// A run that measures its suite aside starts measuring as soon as it
/// owes the measurement, and every step runs beside it until a step that
/// reads it waits for it. The measurement never outlives the loop: when a
/// step ends the invocation, a measurement still under way stops,
/// records nothing, and the next wake takes it again.
async fn drive(ready: &Ready<'_>) -> Result<RunReport, RunError> {
    let Ready {
        ctx,
        root_cancel,
        mode_name,
        policy,
    } = ready;
    let mut aside = super::aside::Aside::new(root_cancel);
    let ended = loop {
        // A Ctrl-C (or any root cancellation) between scheduler
        // steps pauses here; one that lands mid-batch is honored by the
        // per-node child tokens below, whose failed nodes land in the
        // log first and then reach this same check.
        if root_cancel.is_cancelled() {
            aside.stop().await;
            break pause(ctx, PauseReason::Cancelled).await;
        }
        // One read and one replay per iteration: the decision and every
        // handler that needs the run's state read the same derivation.
        let events = ctx.load_events().await?;
        let state = crate::replay::derive(&events);
        if let Some(suite) = schedule::owed_baseline(&state, policy) {
            if policy.measures_aside {
                aside.start(ctx, suite);
            }
        }
        let decision = schedule::decide(&ctx.manifest.workflow, &state, policy);
        if matches!(decision, Decision::MeasureBaseline { .. }) && aside.finish().await? {
            continue;
        }
        match aside
            .beside(take_step(ctx, &state, mode_name, decision))
            .await
        {
            Ok(Some(report)) => break Ok(report),
            Ok(None) => {}
            // A cancellation that reached the step's own git said so on
            // the way out; it is the same Ctrl-C the loop's next turn
            // reads, and it pauses the run the same way.
            Err(RunError::Cancelled) if root_cancel.is_cancelled() => {}
            Err(error) => break Err(error),
        }
    };
    aside.stop().await;
    ended
}

/// Takes one step the scheduler decided, answering with the report of a
/// step that ended the invocation.
async fn take_step(
    ctx: &RunCtx<'_>,
    state: &crate::replay::RunState,
    mode_name: &ModeName,
    decision: Decision,
) -> Result<Option<RunReport>, RunError> {
    match decision {
        Decision::Broken { diagnostic } => Err(steps::broken(ctx, diagnostic).await),
        Decision::MeasureBaseline { suite } => {
            baseline::measure(ctx, suite).await?;
            Ok(None)
        }
        Decision::Finish => steps::run_finished(ctx, mode_name).await.map(Some),
        Decision::Pause { reason } => pause(ctx, reason).await.map(Some),
        Decision::Fail { reason } => steps::run_failed(ctx, reason).await.map(Some),
        Decision::Reroute {
            from,
            to,
            attempt,
            max_reroutes,
            cause,
        } => {
            steps::reroute(ctx, from, to, attempt, max_reroutes, cause).await?;
            Ok(None)
        }
        Decision::GateExhaustedReroutes {
            node,
            goto,
            max_reroutes,
            cause,
        } => steps::gate_exhausted(ctx, state, mode_name, node, goto, max_reroutes, cause).await,
        Decision::EscalateFailure {
            node,
            failure,
            next_attempt,
            continuable,
            grantable,
        } => {
            steps::failure_escalation(
                ctx,
                state,
                node,
                failure,
                (next_attempt, (continuable, grantable)),
            )
            .await
        }
        Decision::Execute(batch) => steps::execute_batch(ctx, state, batch).await,
        Decision::PublishGate { node } => steps::publish_gate(ctx, node).await,
        Decision::PollGate { node, external_ref } => {
            steps::poll_gate(ctx, node, external_ref).await
        }
        Decision::ResolveInternalGate { node } => steps::resolve_internal_gate(ctx, node).await,
        Decision::AskQuestions { node } => steps::ask_questions(ctx, node).await,
        Decision::FinishAnswered { node } => steps::finish_answered(ctx, node).await,
    }
}

/// Wakes the run: builds its context, refuses a run with no log, returns
/// early for one whose log already ends, verifies that the run still
/// holds the artifacts its log accepted and still has the worktree its
/// history describes, records a resume and any deferred registry-write
/// finding, rechecks approved gates, and freezes the mode — everything
/// that happens once per invocation, before the scheduler loop.
async fn start(env: RunEnv<'_>, depth: u32) -> Result<Startup<'_>, RunError> {
    let (ctx, root_cancel, registry_error) = build_ctx(env, depth);

    let view = ctx.run_view().await?;
    if view.events.is_empty() {
        return Err(RunError::UnknownRun {
            run_id: ctx.run_id.clone(),
        });
    }
    if view
        .events
        .iter()
        .any(|e| matches!(e.payload(), Some(EventPayload::Run(RunEvent::Finished(_)))))
    {
        // Re-executing a finished run is a no-op, not an error — the log
        // already has its ending.
        return Ok(Startup::Finished(Box::new(RunReport {
            terminal: RunTerminal::Finished,
            state: view.state,
        })));
    }
    share_dirs(&ctx.manifest.config).await?;
    if view.state.woken() {
        // An invocation already worked on this run — this one is a
        // resume. A birth writes as many events as the run was born
        // holding, so what separates the two is what the log says
        // happened, never how much of it there is.
        super::wake::resume(&ctx, &view).await?;
        // What earlier invocations answered stays answered for the trees
        // they named, as long as the commands still run as they did.
        let woken = ctx.run_view().await?;
        ctx.memo.seed(&woken.events, &woken.state.run);
    }

    // "On wake" means once per invocation, not once per scheduling
    // iteration — done here, before the loop, so both the very first
    // `yunta run` call and every later `yunta resume` do this exactly
    // once. Placed *after* the "already `run_finished`" early return
    // above, deliberately: a genuinely finished run is immutable —
    // nothing reopens it, ever — so a stale approval only matters, and is
    // only ever rechecked, while the run still has unresolved work of its
    // own keeping it open.
    if let Some(error) = registry_error {
        ctx.engine_finding(
            None,
            "engine-registry",
            FindingSeverity::Minor,
            "the process registry could not be written".to_string(),
            Location::run(crate::process_registry::registry_file(), None),
            format!("nothing outside this invocation can see its process tree: {error}"),
        )
        .await?;
    }

    gate_exec::recheck_approved_gates(&ctx, ctx.forge).await?;

    // The mode is frozen once, in `run_created` (`events[0]` — never
    // absent, checked above), and never re-resolved — a resume reads the
    // same name back off the log rather than re-deriving it, same
    // "resolved once, reused forever" discipline runner resolution
    // already follows.
    let mode_name = yunta_core::events::run_mode(&view.events);
    let policy = schedule::Policy::of(ctx.manifest, &mode_name, &view.state.run);
    Ok(Startup::Ready(Box::new(Ready {
        ctx,
        root_cancel,
        mode_name,
        policy,
    })))
}

/// Builds the run's context and the two invocation-scoped values the
/// scheduler loop needs beside it: the loop's own handle on the root
/// cancellation, which the caller always brings, and the
/// process-registry write error,
/// carried past construction because a failed write has no log to record
/// itself on until the context exists.
fn build_ctx(
    env: RunEnv<'_>,
    depth: u32,
) -> (RunCtx<'_>, CancellationToken, Option<std::io::Error>) {
    let RunEnv {
        run_id,
        manifest,
        run_dir,
        worktree,
        adapters,
        storage,
        clock,
        ids,
        human_interaction,
        forge,
        cancel,
        adapter_override,
        ambient,
        secrets,
        observer,
        fence_hook,
    } = env;
    // Resolved once, when the run wakes: every append this invocation
    // makes takes the same values back out, and a run that declares no
    // secret builds an empty one and pays nothing.
    let redactor =
        yunta_core::Redactor::of(&manifest.config.secrets, secrets.as_deref().map(|s| s as _));
    let root_cancel = cancel.clone();
    let root_cancel_for_ctx = root_cancel.clone();
    let (registry, registry_error) = match crate::process_registry::ProcessRegistry::create(
        run_dir,
        Pid::current(),
        clock.now(),
    ) {
        Ok(registry) => (Some(std::sync::Arc::new(registry)), None),
        Err(e) => (None, Some(e)),
    };
    // The per-run MCP host outlives every borrow of this invocation, so
    // it owns a clone of the run's clock and of its observer rather than
    // borrowing them — the one injected clock and the one display
    // surface reach the listener's own event appends.
    let clock_for_host = clock.clone();
    let awake = Arc::new(crate::wakefulness::Wakefulness::new(clock.clone()));
    let observer_for_host = observer.clone();
    // One cache of criterion results per invocation, read by every task
    // cycle and by every check a task session asks for through the host.
    let memo = std::sync::Arc::new(Memo::new(manifest.config_hash.clone()));
    let pool = crate::worktree::CheckoutPool::new(run_dir);
    let registry_for_host = registry.clone();
    let subprocess_vars = subprocess_vars(ambient, &manifest.config);
    let ctx = RunCtx {
        fence_hook,
        run_id,
        manifest,
        run_dir,
        worktree,
        adapters,
        storage,
        clock,
        ids,
        memo: memo.clone(),
        pool: pool.clone(),
        human_interaction,
        adapter_override,
        budget_lifted: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        process_registry: registry,
        root_cancel: root_cancel_for_ctx,
        forge,
        depth,
        ambient,
        subprocess_vars: subprocess_vars.clone().into(),
        secrets,
        redactor: redactor.clone(),
        observer,
        unit: None,
        landing: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        awake,
        // One host per execute_run invocation, shared by every session
        // listener; each of them reads and writes through the host's own
        // clone of the log handle.
        run_tools_host: Arc::new(crate::run_tools::RunToolsHost::new(
            &manifest.workflow,
            crate::run_tools::HostOf {
                storage: storage.clone(),
                run_id: run_id.clone(),
                clock: clock_for_host,
                observer: observer_for_host,
                run_dir: run_dir.to_path_buf(),
                max_artifact_bytes: manifest
                    .config
                    .limits
                    .as_ref()
                    .and_then(|limits| limits.max_artifact_bytes),
                redactor,
                memo,
                pool,
                process_registry: registry_for_host,
                subprocess_vars,
                environment: crate::process::execution_environment(ambient),
                worktree: worktree.to_path_buf(),
            },
        )),
    };
    (ctx, root_cancel, registry_error)
}

/// Every variable layered onto a subprocess of the run: the ambient
/// ones a caller injects, then each shared directory under its variable.
fn subprocess_vars(
    ambient: Option<&yunta_core::Env>,
    config: &yunta_core::ConfigLayer,
) -> Vec<(String, String)> {
    let mut vars = ambient
        .map(|ambient| ambient.subprocess_vars.clone())
        .unwrap_or_default();
    vars.extend(
        config
            .shared_dirs()
            .map(|(var, dir)| (var.to_string(), dir.display().to_string())),
    );
    vars
}

/// Makes every shared directory exist before anything is asked to write
/// in it: a session's sandbox keeps a directory writable, it does not
/// create one.
async fn share_dirs(config: &yunta_core::ConfigLayer) -> Result<(), RunError> {
    for (_, dir) in config.shared_dirs() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|source| RunError::Io {
                context: format!("create the shared directory `{}`", dir.display()),
                source,
            })?;
    }
    Ok(())
}

pub(super) fn find_node<'a>(
    workflow: &'a yunta_core::Workflow,
    node_id: &NodeId,
) -> Result<&'a yunta_core::Node, RunError> {
    workflow
        .nodes
        .iter()
        .find(|n| &n.id == node_id)
        .ok_or_else(|| RunError::Broken {
            diagnostic: format!(
                "scheduler chose node `{node_id}` which the manifest's workflow does not define"
            ),
        })
}
