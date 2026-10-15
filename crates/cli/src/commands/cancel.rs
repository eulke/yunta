//! `yunta cancel <run_id>`: interrupt, then kill — exterminates the
//! run's whole process tree rather than leaving anything orphaned.
//!
//! The channel is `run.dir/scratch/engine.json`: the live engine's pid
//! plus the process-group ids of everything it spawned. Three cases,
//! each explicit:
//!
//! 1. **Engine alive** — SIGINT to the engine pid; its own Ctrl-C path
//!    (interrupt→kill) exterminates the tree and records
//!    `run_paused { reason: "cancelled by user" }`. This command waits
//!    for that terminal in the log, escalating to SIGKILL on the
//!    registered process groups if it doesn't arrive in time.
//! 2. **Engine dead, pgids registered** (a crash) — kill the orphaned
//!    groups directly, record `run_paused { reason: "cancelled after
//!    crash" }`, delete the leftover registry.
//! 3. **No registry yet** — a run its creator has just handed to an engine
//!    moves before that engine registers: wait for the registration, then
//!    case 1. With no engine registering, report what the log says.

use std::time::Duration;

use yunta_core::events::RunPhaseRaw;
use yunta_core::process::group::force_kill_group;
use yunta_core::process::signal::{signal_group, signal_process, Liveness, Signal};
use yunta_core::text::counted;
use yunta_core::{describe, Pid, RunId};
use yunta_engine::pace::Pace;

use crate::commands::advice;
use crate::commands::close::give_back;
use crate::context::{Context, Opened};
use crate::error::{note, warn, CliError, Outcome};
use crate::json::SCHEMA_VERSION;
use crate::output::{emit, progress, Answer, Mode};
use crate::render::doc::{Block, Doc};
use crate::render::ink::Line;
use crate::render::Look;

/// How long the engine gets to react to the SIGINT before the escalation
/// — generous next to the engine's own interrupt grace of at most 200ms,
/// because a mid-batch engine finishes killing its sessions before it
/// pauses.
const ENGINE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);

/// How often `cancel` re-reads the log while it waits for the engine to
/// register, and to record its terminal after the SIGINT.
const ENGINE_SHUTDOWN_POLL: Duration = Duration::from_millis(200);

mod registered;

pub async fn cancel(run_id: &RunId, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let storage = ctx.async_storage().await?;
    let open = ctx.open_run(run_id).await?;

    let state = yunta_engine::derive(&open.events);
    // Where the log leaves the run now: a pause a later wake resumed
    // stopped nothing that is still stopped.
    let parked = matches!(state.run.phase(), RunPhaseRaw::Paused);
    let stopped = parked || matches!(state.run.phase(), RunPhaseRaw::Closed);

    let end = if state.broken.is_some() || stopped {
        let killed = settle_stopped(&ctx, &open, run_id).await;
        CancelState::AlreadyStopped { killed, parked }
    } else {
        stop_engine(&ctx, &storage, &open, run_id, &state, output, look).await?
    };
    emit(output, &answer(run_id, end), look)
}

/// Stops a run whose log says it is moving: signals the engine it runs
/// under and waits for that engine to record its terminal, or, when the
/// engine is gone, settles what it left behind.
async fn stop_engine(
    ctx: &Context,
    storage: &yunta_storage::AsyncStorage,
    open: &Opened,
    run_id: &RunId,
    state: &yunta_engine::RunState,
    output: Mode,
    look: &Look,
) -> Result<CancelState, CliError> {
    // Resolved before anything is signalled or written: a short declared
    // secret leaves the run as it is.
    let redactor = yunta_engine::run_redactor(&open.manifest.doc, &yunta_core::ProcessSecrets)?;
    let Some(registry) = registered::engine_registry(ctx, storage, open, run_id, state).await?
    else {
        return Ok(CancelState::NothingRunning);
    };
    if registry.liveness(&yunta_engine::lock::SystemProbe) == Liveness::Alive {
        // Case 1 — the engine handles the rest itself.
        return signal_live_engine(ctx, storage, output, look, run_id, &registry).await;
    }

    // Case 2 — the engine crashed; its leftovers are ours to clean.
    settle_crash(ctx, storage, run_id, &open.run_dir, &registry, &redactor).await?;
    Ok(CancelState::Crashed {
        engine_pid: registry.engine_pid,
        killed: registry.process_groups.len(),
    })
}

/// Signals the live engine with SIGINT and waits for it to stop the run.
async fn signal_live_engine(
    ctx: &Context,
    storage: &yunta_storage::AsyncStorage,
    output: Mode,
    look: &Look,
    run_id: &RunId,
    registry: &yunta_engine::EngineProcessFile,
) -> Result<CancelState, CliError> {
    progress(
        output,
        look,
        format!(
            "run {}: signalling the live engine (pid {})",
            run_id.handle(),
            registry.engine_pid
        ),
    );
    signal_process(registry.engine_pid, Signal::SIGINT).map_err(|e| {
        CliError::msg(format!(
            "{} — retry `{}`",
            describe(&e),
            advice::cancel(run_id.handle())
        ))
    })?;
    await_engine(ctx, storage, run_id, registry).await
}

/// Waits for the engine a SIGINT went to to record its terminal in the
/// log. Once the shutdown timeout is past, its process groups are killed
/// with SIGKILL, and the engine itself too.
async fn await_engine(
    ctx: &Context,
    storage: &yunta_storage::AsyncStorage,
    run_id: &RunId,
    registry: &yunta_engine::EngineProcessFile,
) -> Result<CancelState, CliError> {
    let called = run_id.handle();
    let engine_pid = registry.engine_pid;
    let deadline = tokio::time::Instant::now() + ENGINE_SHUTDOWN_TIMEOUT;
    loop {
        // A person who stops waiting is a person who stopped
        // waiting: `cancel` is the one command whose whole body is
        // a wait, so it answers to its own interrupt like the run
        // it is stopping answers to the engine's.
        tokio::select! {
            () = Pace::every(ENGINE_SHUTDOWN_POLL).beat() => {}
            () = ctx.cancellation().cancelled() => {
                note(format!(
                    "run {called}: interrupted while waiting — the SIGINT is with the \
                     engine, which stops on its own; `{}` says where it got to",
                    advice::status(run_id.handle())
                ));
                return Ok(CancelState::Interrupted { engine_pid });
            }
        }
        let events = storage.events_for_run(run_id.clone()).await?;
        // The pause this signal asks for, not one a wake has
        // already resumed.
        let state = yunta_engine::derive(&events);
        if !matches!(state.run.phase(), RunPhaseRaw::Open) {
            return Ok(CancelState::Cancelled { engine_pid });
        }
        if tokio::time::Instant::now() >= deadline {
            note(format!(
                "run {called}: the engine did not stop within {} — escalating to SIGKILL \
                 on its process groups",
                crate::render::duration(ENGINE_SHUTDOWN_TIMEOUT)
            ));
            kill_groups(&registry.process_groups);
            if let Err(e) = signal_process(engine_pid, Signal::SIGKILL) {
                if !e.is_gone() {
                    warn(describe(&e));
                }
            }
            return Ok(CancelState::Escalated { engine_pid });
        }
    }
}

/// What `cancel` found and did, as a document carries it.
#[derive(serde::Serialize)]
pub(crate) struct CancelAnswer {
    schema_version: u32,
    run_id: RunId,
    #[serde(flatten)]
    state: CancelState,
}

/// How a cancel ended, and what it did to the processes that were left.
#[derive(serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum CancelState {
    /// The run was stopped already. `killed` process groups its engine
    /// could not stop were killed by this call; `parked` says it waits on a person.
    AlreadyStopped { killed: usize, parked: bool },
    /// No node of the run was in progress, so there was nothing to signal.
    NothingRunning,
    /// The engine recorded its terminal after the interrupt it was sent.
    Cancelled { engine_pid: Pid },
    /// The engine did not stop within the shutdown timeout, so its process
    /// groups were killed.
    Escalated { engine_pid: Pid },
    /// The wait was interrupted; the interrupt stays with the engine.
    Interrupted { engine_pid: Pid },
    /// The engine had died: its orphaned process groups were killed, and the
    /// pause recorded.
    Crashed { engine_pid: Pid, killed: usize },
}

fn answer(run_id: &RunId, state: CancelState) -> CancelAnswer {
    CancelAnswer {
        schema_version: SCHEMA_VERSION,
        run_id: run_id.clone(),
        state,
    }
}

impl Answer for CancelAnswer {
    fn document<'a>(&'a self, _look: &Look) -> Doc<'a> {
        let called = self.run_id.handle();
        let said = match &self.state {
            CancelState::AlreadyStopped { killed, .. } if *killed > 0 => format!(
                "run {called}: already stopped — killed {}, which the engine could not stop",
                counted(*killed, "process group")
            ),
            CancelState::AlreadyStopped { parked: true, .. } => format!(
                "run {called}: already stopped — nothing to cancel; `{}` closes it for good",
                advice::close(called)
            ),
            CancelState::AlreadyStopped { .. } => {
                format!("run {called}: already stopped — nothing to cancel")
            }
            CancelState::NothingRunning => {
                format!("run {called}: no node in progress — nothing to cancel")
            }
            CancelState::Cancelled { .. } => {
                format!("run {called}: cancelled — the log has its terminal")
            }
            CancelState::Crashed { engine_pid, killed } => format!(
                "run {called}: engine (pid {engine_pid}) was already dead — killed {}, \
                 recorded the pause",
                counted(*killed, "orphaned process group")
            ),
            // A stop the engine answers to has no line of its own: its
            // notes went to stderr while it ran.
            CancelState::Escalated { .. } | CancelState::Interrupted { .. } => return Doc::new(),
        };
        Doc::new().with(Block::Lines(vec![Line::new().plain(said)]))
    }

    fn outcome(&self) -> Outcome {
        match &self.state {
            CancelState::Escalated { .. } => Outcome::Reported,
            _ => Outcome::Success,
        }
    }
}

/// Kills the process groups a stopped run's engine could not stop, and gives
/// back the checkouts they held. Runs when `cancel` finds the run paused or
/// closed, or its log broken. Each group its registry names is killed, unless
/// [`may_signal`] refuses it. When any of them is alive, the checkouts go
/// back to the pool, as `close` gives them back; the run's own state is left
/// as it is. Answers how many of the named groups are alive as it looks.
async fn settle_stopped(ctx: &Context, open: &Opened, run_id: &RunId) -> usize {
    let killed = kill_unstopped_groups(&open.run_dir).await;
    if killed > 0 {
        give_back(ctx, open, run_id).await;
    }
    killed
}

/// What a run whose engine died left behind, settled: every process
/// group the engine registered killed, the run's pause recorded, and the
/// registry naming the dead engine removed. The `run_paused` is emitted
/// through the engine, not hand-built here, so the CLI never stamps an
/// event with a clock of its own. `redactor` is the run's own, resolved by
/// the caller before anything here runs.
pub(crate) async fn settle_crash(
    ctx: &Context,
    storage: &yunta_storage::AsyncStorage,
    run_id: &RunId,
    run_dir: &std::path::Path,
    registry: &yunta_engine::EngineProcessFile,
    redactor: &yunta_core::Redactor,
) -> Result<(), CliError> {
    kill_groups(&registry.process_groups);
    yunta_engine::record_pause_after_crash(
        storage,
        run_id,
        &yunta_core::events::PauseReason::CancelledAfterCrash,
        &ctx.clock,
        redactor,
    )
    .await?;
    if let Err(e) = tokio::fs::remove_file(yunta_engine::registry_path(run_dir)).await {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn(format!("could not delete engine.json: {e}"));
        }
    }
    Ok(())
}

/// SIGKILL to every process group the engine registered. A group that is
/// already gone is the end state wanted; any other refusal is printed,
/// never hidden. A pgid that isn't a real run process group (see
/// [`signalable_group`]) is skipped with a loud note rather than signalled.
fn kill_groups(groups: &[Pid]) {
    for pgid in groups {
        if !may_signal(*pgid) {
            continue;
        }
        if let Err(e) = signal_group(*pgid, Signal::SIGKILL) {
            warn(describe(&e));
        }
    }
}

/// Whether `pgid` may be signalled as a run's group. One that may not is
/// warned about, and skipped.
fn may_signal(pgid: Pid) -> bool {
    if signalable_group(pgid) {
        return true;
    }
    warn(format!(
        "refusing to signal process group {pgid}: not a run's own process group — \
         engine.json is corrupt, skipping it"
    ));
    false
}

/// SIGKILL to every process group a stopped run's registry names, as the crash
/// path does: the groups its engine could not stop, which hold the run's
/// checkout taken while they run. A group whose leader has exited is signalled
/// too, since its members may still run. Each group is killed until none of its
/// members can run. Answers how many of the named groups still answered when it
/// looked.
async fn kill_unstopped_groups(run_dir: &std::path::Path) -> usize {
    let yunta_engine::Registry::Read(registry) = yunta_engine::read_registry(run_dir) else {
        return 0;
    };
    let groups = &registry.doc.process_groups;
    let running = answering(groups);
    for pgid in groups.iter().copied().filter(|pgid| may_signal(*pgid)) {
        if let Err(e) = force_kill_group(pgid).await {
            warn(describe(&e));
        }
    }
    running
}

/// How many of `groups` still run, read as the engine reads them: a group
/// whose leader exited while a child it left runs still counts.
fn answering(groups: &[Pid]) -> usize {
    yunta_engine::running_groups(groups).len()
}

/// Whether `cancel` may signal a process group. `signal_group` is
/// `killpg(pgid)` = `kill(-pgid)`, so `pgid` of 1 becomes `kill(-1)` —
/// every process this user can signal, not one run's tree. The [`Pid`]
/// type already makes 0 and negatives unrepresentable, so this rejects the
/// one dangerous value that survives it: init's group (pgid 1). A registry
/// carrying it is corrupt, and the safe reading is to skip it.
fn signalable_group(pgid: Pid) -> bool {
    pgid.as_u32() > 1
}

#[cfg(test)]
mod tests {
    use super::{answer, signalable_group, CancelState};
    use crate::error::Outcome;
    use crate::output::Answer;
    use crate::render::{draw, Look};
    use yunta_core::{Pid, RunId};

    #[test]
    fn a_run_never_signals_init_or_a_lower_group() {
        // pgid 1 -> killpg(1) -> kill(-1): every process, not one run's
        // tree. `Pid` forbids 0 and negatives, so 1 is the sole dangerous
        // value it still admits — and the one this must reject.
        assert!(!signalable_group(Pid::try_from(1u32).unwrap()));
        assert!(signalable_group(Pid::try_from(2u32).unwrap()));
        assert!(signalable_group(Pid::try_from(4321u32).unwrap()));
    }

    #[test]
    fn a_cancel_that_finds_nothing_to_stop_says_so_in_its_words() {
        let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
        let said = |state: CancelState| {
            let reply = answer(&run, state);
            draw(reply.document(&Look::plain()), &Look::plain())
        };
        assert_eq!(
            said(CancelState::NothingRunning),
            "run AAAAAA: no node in progress — nothing to cancel\n"
        );
        assert_eq!(
            said(CancelState::AlreadyStopped {
                killed: 2,
                parked: false
            }),
            "run AAAAAA: already stopped — killed 2 process groups, which the engine could not stop\n"
        );
        assert_eq!(
            said(CancelState::AlreadyStopped {
                killed: 0,
                parked: true
            }),
            "run AAAAAA: already stopped — nothing to cancel; `yunta close AAAAAA` closes it for good\n"
        );
        assert_eq!(
            said(CancelState::AlreadyStopped {
                killed: 0,
                parked: false
            }),
            "run AAAAAA: already stopped — nothing to cancel\n"
        );
    }

    #[test]
    fn a_cancel_that_waits_on_its_engine_says_what_it_found() {
        let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
        let engine = Pid::try_from(4321u32).unwrap();
        let said = |state: CancelState| {
            let reply = answer(&run, state);
            draw(reply.document(&Look::plain()), &Look::plain())
        };
        assert_eq!(
            said(CancelState::Cancelled { engine_pid: engine }),
            "run AAAAAA: cancelled — the log has its terminal\n"
        );
        assert_eq!(
            said(CancelState::Crashed {
                engine_pid: engine,
                killed: 1
            }),
            "run AAAAAA: engine (pid 4321) was already dead — killed 1 orphaned process group, recorded the pause\n"
        );
        // A stop the engine answers to draws no line of its own.
        assert_eq!(said(CancelState::Escalated { engine_pid: engine }), "");
        assert_eq!(said(CancelState::Interrupted { engine_pid: engine }), "");
        assert_eq!(
            answer(&run, CancelState::Escalated { engine_pid: engine }).outcome(),
            Outcome::Reported
        );
    }
}
