//! The run execution driver: the scheduler loop that replays the log,
//! asks [`schedule::decide`] what is next, and runs it until the answer
//! is terminal — plus the pause paths every stop funnels through and the
//! node lookup the step handlers share.
//!
//! Crash, restart and Ctrl-C are the same case: whatever the log says
//! happened, happened; everything else re-runs. A resume re-enters
//! [`execute_run_at_depth`] and reads what remains off the log, never from
//! in-process state.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{
    EventDraft, EventPayload, FindingSeverity, PauseReason, RunMovedPayload, RunPausedPayload,
};
use yunta_core::{Clock, ModeName, NodeId, RunId};
use yunta_storage::AsyncStorage;

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
        report = Box::pin(drive_where_it_works(&ready)) => report,
        stopped = ready.ctx.awake.watch(&log) => match stopped {
            Ok(never) => match never {},
            Err(error) => Err(error.into()),
        },
    }
}

/// Where one drive of the scheduler loop left the run. The report is
/// boxed, as a run's startup is: a move carries a path.
enum Driven {
    /// At a stop, which the invocation reports.
    Stopped(Box<RunReport>),
    /// In another checkout, where the loop drives it on.
    Moved(PathBuf),
}

impl Driven {
    fn stopped(report: RunReport) -> Self {
        Driven::Stopped(Box::new(report))
    }
}

/// Drives the run in the checkout it works in, and on in each one it
/// moves into: what the invocation holds carries over every move.
async fn drive_where_it_works(ready: &Ready<'_>) -> Result<RunReport, RunError> {
    let mut moved: Option<PathBuf> = None;
    loop {
        let driven = {
            let there = moved
                .as_deref()
                .map(|checkout| ready.ctx.moved_to(checkout));
            Box::pin(drive(there.as_ref().unwrap_or(&ready.ctx), ready)).await?
        };
        match driven {
            Driven::Stopped(report) => return Ok(*report),
            Driven::Moved(checkout) => moved = Some(checkout),
        }
    }
}

/// The scheduler loop: replays the log, asks what is next, and runs it
/// until the answer is terminal — or until the run moves into the
/// checkout its last unit built in, where it goes on.
///
/// A run that measures its suite aside starts measuring as soon as it
/// owes the measurement, and every step runs beside it until a step that
/// reads it waits for it. The measurement never outlives the loop: when a
/// step ends the invocation, a measurement still under way stops,
/// records nothing, and the next wake takes it again. A run moves only
/// while nothing measures, so no move stops one.
async fn drive(ctx: &RunCtx<'_>, ready: &Ready<'_>) -> Result<Driven, RunError> {
    let Ready {
        root_cancel,
        mode_name,
        policy,
        ..
    } = ready;
    let mut aside = super::aside::Aside::new(root_cancel);
    let ended = loop {
        // A Ctrl-C (or any root cancellation) between scheduler
        // steps pauses here; one that lands mid-batch is honored by the
        // per-node child tokens below, whose failed nodes land in the
        // log first and then reach this same check.
        if root_cancel.is_cancelled() {
            aside.stop().await;
            break pause(ctx, PauseReason::Cancelled)
                .await
                .map(Driven::stopped);
        }
        // One read and one replay per iteration: the decision and every
        // handler that needs the run's state read the same derivation.
        let events = ctx.load_events().await?;
        let state = crate::replay::derive(&events);
        if let Some(owed) = schedule::owed_baseline(&state, policy) {
            if policy.measures_aside {
                aside.start(ctx, owed);
            }
        }
        let decision = schedule::decide(&ctx.manifest.workflow, &state, policy);
        if matches!(decision, Decision::MeasureBaseline { .. }) && aside.finish().await? {
            continue;
        }
        if let Some(checkout) = moves_before(ctx, &state, policy, &aside, &decision).await? {
            break Ok(Driven::Moved(checkout));
        }
        match aside
            .beside(take_step(ctx, &state, mode_name, decision))
            .await
        {
            Ok(Some(report)) => break Ok(Driven::stopped(report)),
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

/// Moves the run, before it executes `decision`, into a checkout of its
/// pool that already built the commit it stands at — the one its last
/// unit worked in — when it is the moment to look for one, and says so on
/// the log. Answers where the run works now; `None` when it stays.
async fn moves_before(
    ctx: &RunCtx<'_>,
    state: &crate::replay::RunState,
    policy: &schedule::Policy,
    aside: &super::aside::Aside<'_>,
    decision: &Decision,
) -> Result<Option<PathBuf>, RunError> {
    let Decision::Execute(batch) = decision else {
        return Ok(None);
    };
    if !aside.idle() || !schedule::follows_build(&ctx.manifest.workflow, state, policy, batch) {
        return Ok(None);
    }
    let supervision = ctx.root_supervision();
    // Boxed: the move runs at the bottom of every composed run's stack.
    let moved = Box::pin(ctx.pool.follow_build(ctx.run_id, ctx.worktree, supervision)).await?;
    let Some(checkout) = moved else {
        return Ok(None);
    };
    let payload = RunMovedPayload {
        checkout: checkout.clone(),
    };
    ctx.emit(None, EventPayload::Run(RunEvent::Moved(payload)))
        .await?;
    Ok(Some(checkout))
}

/// Takes one step the scheduler decided, answering with the report of a
/// step that ended the invocation. A step the disk has no room for stops
/// the run, saying how to make some.
async fn take_step(
    ctx: &RunCtx<'_>,
    state: &crate::replay::RunState,
    mode_name: &ModeName,
    decision: Decision,
) -> Result<Option<RunReport>, RunError> {
    match step(ctx, state, mode_name, decision).await {
        Err(RunError::NoRoom(crate::worktree::WorktreeError::NoRoom { room })) => {
            pause(ctx, room.pause()).await.map(Some)
        }
        taken => taken,
    }
}

/// What a step answers: the report of a step that ended the invocation.
type Stepped<'s> = Pin<Box<dyn Future<Output = Result<Option<RunReport>, RunError>> + 's>>;

/// The step the scheduler decided, boxed apart by its kind: a run nested
/// inside another run's step carries the frame of the one step it takes,
/// not one sized for every kind of step, at every level of the
/// composition.
fn step<'s>(
    ctx: &'s RunCtx<'_>,
    state: &'s crate::replay::RunState,
    mode: &'s ModeName,
    decision: Decision,
) -> Stepped<'s> {
    match decision {
        Decision::Broken { diagnostic } => Box::pin(fails(steps::broken(ctx, diagnostic))),
        Decision::MeasureBaseline { owed } => Box::pin(goes_on(baseline::measure(ctx, owed))),
        Decision::Finish => Box::pin(ends(steps::run_finished(ctx, mode))),
        Decision::Pause { reason } => Box::pin(ends(pause(ctx, reason))),
        Decision::Fail { reason } => Box::pin(ends(steps::run_failed(ctx, reason))),
        Decision::Reroute {
            from,
            to,
            attempt,
            max_reroutes,
            cause,
        } => {
            let rerouted = steps::reroute(ctx, from, to, attempt, max_reroutes, cause);
            Box::pin(goes_on(rerouted))
        }
        Decision::GateExhaustedReroutes {
            node,
            goto,
            max_reroutes,
            cause,
        } => {
            let exhausted =
                steps::gate_exhausted(ctx, state, mode, node, goto, max_reroutes, cause);
            Box::pin(exhausted)
        }
        Decision::EscalateFailure {
            node,
            failure,
            next_attempt,
            continuable,
            grantable,
        } => {
            let next = (next_attempt, (continuable, grantable));
            Box::pin(steps::failure_escalation(ctx, state, node, failure, next))
        }
        Decision::Execute(batch) => Box::pin(steps::execute_batch(ctx, state, batch)),
        Decision::PublishGate { node } => Box::pin(steps::publish_gate(ctx, node)),
        Decision::PollGate { node, external_ref } => {
            Box::pin(steps::poll_gate(ctx, node, external_ref))
        }
        Decision::ResolveInternalGate { node } => Box::pin(steps::resolve_internal_gate(ctx, node)),
        Decision::AskQuestions { node } => Box::pin(steps::ask_questions(ctx, node)),
        Decision::FinishAnswered { node } => Box::pin(steps::finish_answered(ctx, node)),
    }
}

/// A step that finds the run broken, saying how.
async fn fails(broken: impl Future<Output = RunError>) -> Result<Option<RunReport>, RunError> {
    Err(broken.await)
}

/// A step that ends the invocation, reporting what `ended` reports.
async fn ends(
    ended: impl Future<Output = Result<RunReport, RunError>>,
) -> Result<Option<RunReport>, RunError> {
    ended.await.map(Some)
}

/// A step after which the run goes on.
async fn goes_on(
    step: impl Future<Output = Result<(), RunError>>,
) -> Result<Option<RunReport>, RunError> {
    step.await.map(|()| None)
}

/// Wakes the run: builds its context, refuses a run with no log, returns
/// early for one whose log already ends, verifies that the run still
/// holds the artifacts its log accepted and still has the worktree its
/// history describes, records a resume and any deferred registry-write
/// finding, rechecks approved gates, and freezes the mode — everything
/// that happens once per invocation, before the scheduler loop.
async fn start(env: RunEnv<'_>, depth: u32) -> Result<Startup<'_>, RunError> {
    let (ctx, root_cancel, registry_error) = super::invocation::build_ctx(env, depth);

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
    super::invocation::share_dirs(&ctx.manifest.config).await?;
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
