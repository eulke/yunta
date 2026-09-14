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
    EventDraft, EventPayload, FindingSeverity, PauseReason, RunPausedPayload, RunResumedPayload,
};
use yunta_core::{Clock, ModeName, NodeId, Pid, RunId};
use yunta_storage::AsyncStorage;

use crate::artifacts::ArtifactIntegrity;
use crate::replay::RunView;
use crate::task_cycle::Memo;
use crate::worktree::{RunWorktree, WorktreeIntegrity};

use super::schedule::{self, Decision};
use super::{gate_exec, steps, RunCtx, RunEnv, RunError, RunReport, RunTerminal};
use yunta_core::events::RunEvent;

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
enum Startup<'a> {
    Finished(RunReport),
    Ready {
        ctx: RunCtx<'a>,
        root_cancel: CancellationToken,
        mode_name: ModeName,
        policy: schedule::Policy,
    },
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
    let (ctx, root_cancel, mode_name, policy) = match start(env, depth).await? {
        Startup::Finished(report) => return Ok(report),
        Startup::Ready {
            ctx,
            root_cancel,
            mode_name,
            policy,
        } => (ctx, root_cancel, mode_name, policy),
    };

    loop {
        // A Ctrl-C (or any root cancellation) between scheduler
        // steps pauses here; one that lands mid-batch is honored by the
        // per-node child tokens below, whose failed nodes land in the
        // log first and then reach this same check.
        if root_cancel.is_cancelled() {
            return pause(&ctx, PauseReason::Cancelled).await;
        }
        // One read and one replay per iteration: the decision and every
        // handler that needs the run's state read the same derivation.
        let events = ctx.load_events().await?;
        let state = crate::replay::derive(&events);
        match schedule::decide(&ctx.manifest.workflow, &state, &policy) {
            Decision::Broken { diagnostic } => return Err(steps::broken(&ctx, diagnostic).await),
            Decision::Finish => return steps::run_finished(&ctx, &mode_name).await,
            Decision::Pause { reason } => return pause(&ctx, reason).await,
            Decision::Fail { reason } => return steps::run_failed(&ctx, reason).await,
            Decision::Reroute {
                from,
                to,
                attempt,
                max_reroutes,
                cause,
            } => steps::reroute(&ctx, from, to, attempt, max_reroutes, cause).await?,
            Decision::GateExhaustedReroutes {
                node,
                goto,
                max_reroutes,
                cause,
            } => {
                if let Some(report) =
                    steps::gate_exhausted(&ctx, &state, &mode_name, node, goto, max_reroutes, cause)
                        .await?
                {
                    return Ok(report);
                }
            }
            Decision::Execute(batch) => {
                if let Some(report) = steps::execute_batch(&ctx, &state, batch).await? {
                    return Ok(report);
                }
            }
            Decision::PublishGate { node } => {
                if let Some(report) = steps::publish_gate(&ctx, node).await? {
                    return Ok(report);
                }
            }
            Decision::PollGate { node, external_ref } => {
                if let Some(report) = steps::poll_gate(&ctx, node, external_ref).await? {
                    return Ok(report);
                }
            }
            Decision::ResolveInternalGate { node } => {
                if let Some(report) = steps::resolve_internal_gate(&ctx, node).await? {
                    return Ok(report);
                }
            }
            Decision::AskQuestions { node } => {
                if let Some(report) = steps::ask_questions(&ctx, node).await? {
                    return Ok(report);
                }
            }
            Decision::FinishAnswered { node } => {
                if let Some(report) = steps::finish_answered(&ctx, node).await? {
                    return Ok(report);
                }
            }
        }
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
        return Ok(Startup::Finished(RunReport {
            terminal: RunTerminal::Finished,
            state: view.state,
        }));
    }
    if view.events.len() > 1 {
        // Anything beyond run_created means a previous invocation worked
        // on this run — this one is a resume.
        resume(&ctx, &view).await?;
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
            crate::process_registry::registry_path(ctx.run_dir)
                .display()
                .to_string(),
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
    let policy = schedule::Policy::of(ctx.manifest, &mode_name);
    Ok(Startup::Ready {
        ctx,
        root_cancel,
        mode_name,
        policy,
    })
}

/// Wakes a run that already has history: verifies that the run is still
/// what its own log says it is, records the resume with the policy each
/// orphan node resolves to, and states what the verification could not
/// check.
///
/// The order is the whole of it. A run that cannot answer for its history
/// is broken with a diagnostic before anything says it resumed, never a
/// run that keeps going and hands a node something the log never saw.
/// What the verification could not do is said after, because by then the
/// run has woken.
async fn resume(ctx: &RunCtx<'_>, view: &RunView) -> Result<(), RunError> {
    let artifacts = verify_before_waking(ctx, view).await?;
    record_resume(ctx, view).await?;
    if let Some(detail) = artifacts.unverifiable_detail() {
        ctx.engine_finding(
            None,
            "engine-artifact-store",
            FindingSeverity::Minor,
            "the run holds artifacts this binary cannot verify".to_string(),
            ctx.run_dir
                .join(crate::artifacts::store::OBJECTS_DIR)
                .display()
                .to_string(),
            detail,
        )
        .await?;
    }
    Ok(())
}

/// Asks the run's two questions about itself before it wakes, and returns
/// what the artifact half found so the caller can report what it could
/// not check.
///
/// The two ask different questions of different things, and the
/// difference is deliberate. An artifact is immutable, so it is verified
/// by content: every object the log names is read back and hashed against
/// its own name. A worktree is the work and changes by design, so only
/// its identity and its ancestry are verified — see [`WorktreeIntegrity`]
/// for why its content is not, and what the engine does instead with a
/// tree that moved.
///
/// A missing worktree is the one failure here that is not `broken`: the
/// run's own evidence is intact, so the error carries the remedy instead.
async fn verify_before_waking(
    ctx: &RunCtx<'_>,
    view: &RunView,
) -> Result<ArtifactIntegrity, RunError> {
    let artifacts = ArtifactIntegrity::of(ctx.run_dir, &view.events).await;
    if let Some(diagnostic) = artifacts.diagnostic(ctx.run_id) {
        return Err(steps::broken(ctx, diagnostic).await);
    }
    let worktree = WorktreeIntegrity::of(
        RunWorktree {
            run_id: ctx.run_id,
            path: ctx.worktree,
            base_commit: &ctx.manifest.base_commit,
            isolation: ctx.manifest.isolation,
        },
        ctx.root_supervision(),
    )
    .await?;
    if let Some(diagnostic) = worktree.diagnostic() {
        return Err(steps::broken(ctx, diagnostic).await);
    }
    Ok(artifacts)
}

/// Writes the `run_resumed` that says the run woke, carrying the policy
/// each orphan node resolves to. Whether they share one is the
/// payload's own arithmetic.
async fn record_resume(ctx: &RunCtx<'_>, view: &RunView) -> Result<(), RunError> {
    // A node the mode excludes never ran, so the orphans are the same
    // whichever nodes are in the mode.
    let policies = schedule::resume_policies(
        ctx.manifest.workflow.nodes.iter(),
        &view.state,
        ctx.manifest.config.resolved_on_interrupt(),
    );
    ctx.emit(
        None,
        EventPayload::Run(RunEvent::Resumed(RunResumedPayload::new(policies))),
    )
    .await?;
    Ok(())
}

/// Builds the run's context and the two invocation-scoped values the
/// scheduler loop needs beside it: the loop's own handle on the root
/// cancellation (`None` — tests, callers with no signal source — gets a
/// token nothing ever fires), and the process-registry write error,
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
        max_task_retries,
        human_interaction,
        forge,
        cancel,
        adapter_override,
        ambient,
        secrets,
        observer,
    } = env;
    let root_cancel = cancel.cloned().unwrap_or_default();
    let root_cancel_for_ctx = root_cancel.clone();
    let (registry, registry_error) = match crate::process_registry::ProcessRegistry::create(
        run_dir,
        Pid::current(),
        clock.now().to_rfc3339(),
    ) {
        Ok(registry) => (Some(registry), None),
        Err(e) => (None, Some(e)),
    };
    // The per-run MCP host outlives every borrow of this invocation, so
    // it owns a clone of the run's clock and of its observer rather than
    // borrowing them — the one injected clock and the one display
    // surface reach the listener's own event appends.
    let clock_for_host = clock.clone();
    let observer_for_host = observer.clone();
    let ctx = RunCtx {
        run_id,
        manifest,
        run_dir,
        worktree,
        adapters,
        storage,
        clock,
        ids,
        max_task_retries,
        memo: Memo::new(manifest.config_hash.clone()),
        human_interaction,
        adapter_override,
        budget_lifted: std::sync::atomic::AtomicBool::new(false),
        process_registry: registry,
        root_cancel: root_cancel_for_ctx,
        forge,
        depth,
        ambient,
        secrets,
        observer,
        // One host per execute_run invocation, shared by every session
        // listener; each of them reads and writes through the host's own
        // clone of the log handle.
        run_tools_host: Arc::new(crate::run_tools::RunToolsHost::new(
            storage.clone(),
            run_id.clone(),
            &manifest.workflow,
            clock_for_host,
            observer_for_host,
            run_dir.to_path_buf(),
            manifest
                .config
                .limits
                .as_ref()
                .and_then(|limits| limits.max_artifact_bytes),
        )),
    };
    (ctx, root_cancel, registry_error)
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
