//! The run execution driver: the scheduler loop that replays the log,
//! asks [`schedule::next_step`] what is next, and runs it until the answer
//! is terminal — plus the pause paths every stop funnels through and the
//! node lookup the step handlers share.
//!
//! Crash, restart and Ctrl-C are the same case: whatever the log says
//! happened, happened; everything else re-runs. A resume re-enters
//! [`execute_run_at_depth`] and reads what remains off the log, never from
//! in-process state.

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{
    EventDraft, EventPayload, FindingSeverity, RunPausedPayload, RunResumedPayload,
};
use yunta_core::{Clock, ModeName, NodeId, Pid, RunId};
use yunta_storage::AsyncStorage;

use crate::task_cycle::Memo;

use super::schedule::{self, ScheduleStep};
use super::{gate_exec, steps, RunCtx, RunEnv, RunError, RunReport, RunTerminal};

/// Records the `run_paused` a post-crash `yunta cancel` writes when it
/// finds the engine already dead. The CLI never builds an `EventDraft`
/// itself: event construction and its clock stamp live here, so every
/// event on the log is emitted by the engine through one injected clock.
/// The run-level `run_paused` event — built in one place so every path that
/// stops a run (the scheduler's [`record_pause`], a crash's
/// [`record_pause_after_crash`]) writes the same event.
fn run_paused(reason: &str) -> EventPayload {
    EventPayload::RunPaused(RunPausedPayload {
        reason: reason.to_string(),
    })
}

pub async fn record_pause_after_crash(
    storage: &AsyncStorage,
    run_id: &RunId,
    reason: &str,
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
pub(super) async fn pause(ctx: &RunCtx<'_>, reason: String) -> Result<RunReport, RunError> {
    record_pause(ctx, &reason).await?;
    Ok(RunReport {
        terminal: RunTerminal::Paused { reason },
        state: ctx.run_view().await?.state,
    })
}

/// Emits the run-level `run_paused` event and exports the forensic log — the
/// one place the pause event is written, shared by the scheduler's own pause
/// and the gate flow's.
pub(super) async fn record_pause(ctx: &RunCtx<'_>, reason: &str) -> Result<(), RunError> {
    ctx.emit(None, run_paused(reason)).await?;
    ctx.export_events_jsonl().await
}

/// Everything the scheduler loop runs on, once the run has woken: its
/// built context, the loop's own handle on the root cancellation, and the
/// frozen mode. `Finished` short-circuits a run whose log already ends.
enum Startup<'a> {
    Finished(RunReport),
    Ready {
        ctx: RunCtx<'a>,
        root_cancel: CancellationToken,
        mode_name: ModeName,
        mode_nodes: Option<HashSet<NodeId>>,
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
    let (ctx, root_cancel, mode_name, mode_nodes) = match start(env, depth).await? {
        Startup::Finished(report) => return Ok(report),
        Startup::Ready {
            ctx,
            root_cancel,
            mode_name,
            mode_nodes,
        } => (ctx, root_cancel, mode_name, mode_nodes),
    };

    loop {
        // A Ctrl-C (or any root cancellation) between scheduler
        // steps pauses here; one that lands mid-batch is honored by the
        // per-node child tokens below, whose failed nodes land in the
        // log first and then reach this same check.
        if root_cancel.is_cancelled() {
            return pause(&ctx, "cancelled by user".to_string()).await;
        }
        let events = ctx.load_events().await?;
        match schedule::next_step(
            &ctx.manifest.workflow,
            &events,
            ctx.manifest.max_parallel_nodes,
            ctx.manifest.config.resolved_on_interrupt(),
            ctx.manifest.config.resolved_on_failure(),
            mode_nodes.as_ref(),
        ) {
            ScheduleStep::Broken { diagnostic } => {
                return Err(steps::broken(&ctx, diagnostic).await)
            }
            ScheduleStep::Finish => return steps::finish(&ctx, &mode_name).await,
            ScheduleStep::Pause { reason } => return pause(&ctx, reason).await,
            ScheduleStep::Fail { reason } => return steps::run_failed(&ctx, reason).await,
            ScheduleStep::Reroute {
                from,
                to,
                attempt,
                max_reroutes,
                cause,
            } => steps::reroute(&ctx, from, to, attempt, max_reroutes, cause).await?,
            ScheduleStep::GateExhaustedReroutes {
                node,
                goto,
                max_reroutes,
                cause,
            } => {
                if let Some(report) = steps::gate_exhausted(
                    &ctx,
                    &events,
                    &mode_name,
                    node,
                    goto,
                    max_reroutes,
                    cause,
                )
                .await?
                {
                    return Ok(report);
                }
            }
            ScheduleStep::Execute(batch) => {
                if let Some(report) = steps::execute_batch(&ctx, &events, batch).await? {
                    return Ok(report);
                }
            }
            ScheduleStep::PublishGate { node } => {
                if let Some(report) = steps::publish_gate(&ctx, node).await? {
                    return Ok(report);
                }
            }
            ScheduleStep::PollGate { node, external_ref } => {
                if let Some(report) = steps::poll_gate(&ctx, node, external_ref).await? {
                    return Ok(report);
                }
            }
            ScheduleStep::ResolveInternalGate { node } => {
                if let Some(report) = steps::resolve_internal_gate(&ctx, node).await? {
                    return Ok(report);
                }
            }
            ScheduleStep::AskQuestions { node } => {
                if let Some(report) = steps::ask_questions(&ctx, node).await? {
                    return Ok(report);
                }
            }
        }
    }
}

/// Wakes the run: builds its context, refuses a run with no log, returns
/// early for one whose log already ends, records a resume and any
/// deferred registry-write finding, rechecks approved gates, and freezes
/// the mode — everything that happens once per invocation, before the
/// scheduler loop.
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
        .any(|e| matches!(e.payload(), Some(EventPayload::RunFinished(_))))
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
        // on this run — this one is a resume. A node the mode excludes
        // never ran, so the orphans are the same whichever nodes are
        // in the mode.
        let policies = schedule::resume_policies(
            ctx.manifest.workflow.nodes.iter(),
            &view.state,
            ctx.manifest.config.resolved_on_interrupt(),
        );
        let shared: BTreeSet<&str> = policies
            .iter()
            .map(|policy| policy.on_interrupt.as_str())
            .collect();
        let resume_policy_applied = match shared.iter().next() {
            Some(policy) if shared.len() == 1 => Some((*policy).to_string()),
            _ => None,
        };
        ctx.emit(
            None,
            EventPayload::RunResumed(RunResumedPayload {
                resume_policy_applied,
                policies,
            }),
        )
        .await?;
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
    let mode_nodes = crate::modes::mode_included_nodes(&ctx.manifest.workflow, &mode_name);
    Ok(Startup::Ready {
        ctx,
        root_cancel,
        mode_name,
        mode_nodes,
    })
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
