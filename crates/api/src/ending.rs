//! See [`crate`]. A run's end: cancelled, its whole process tree stopped
//! rather than left orphaned, or closed for good.
//!
//! A cancel reaches a run through `run.dir/scratch/engine.json`: the live
//! engine's pid plus the process-group ids of everything it spawned. Three
//! cases, each explicit:
//!
//! 1. **Engine alive**: SIGINT to the engine pid; its own interrupt path
//!    stops the tree and records `run_paused` with `PauseReason::Cancelled`.
//!    The cancel waits for that terminal on the log, escalating to SIGKILL on
//!    the registered process groups if it does not arrive in time.
//! 2. **Engine dead, groups registered** (a crash): what the dead engine left
//!    is closed in a session's own order (`groups`), then `run_paused` is
//!    recorded with `PauseReason::CancelledAfterCrash` and the leftover
//!    registry deleted.
//! 3. **No registry yet**: a run its creator has just handed to an engine
//!    moves before that engine registers, so the cancel waits for the
//!    registration (`registered`), then case 1. With no engine registering,
//!    nothing was running.
//!
//! A run its log says stopped has the groups its engine could not stop
//! killed, and its checkouts given back.
//!
//! A close ends a stopped run nobody is going to continue, as `cancelled` by
//! who closed it, so it stops waiting on a person. A run whose engine died is
//! settled first, the way a cancel settles one. Its checkouts go back to the
//! project's pool; its branches stay for a person to read and for `gc` to
//! remove.
//!
//! What either says while it works reaches the caller as a [`Remark`], in
//! the order it is met; nothing here prints.

mod groups;
mod registered;

use std::path::Path;
use std::time::Duration;

use surco::words::Standing;
use yunta_core::events::{PauseReason, RunPhaseRaw};
use yunta_core::pace::Pace;
use yunta_core::process::signal::{signal_process, Liveness, Signal};
use yunta_core::{describe, Isolation, Redactor, Responder, RunId};
use yunta_engine::lock::SystemProbe;
use yunta_engine::{CloseRunError, EngineProcessFile, Registry};
use yunta_render::page::CancelState;
use yunta_render::refusal::Refused;
use yunta_replay::registry::Engine;
use yunta_replay::RunState;
use yunta_storage::AsyncStorage;

use crate::{Error, Failure, OpenedRun, Remark, Remarks, Stop};

/// How long the engine gets to react to the SIGINT before the escalation:
/// generous next to the engine's own interrupt grace of at most 200ms,
/// because a mid-batch engine finishes killing its sessions before it
/// pauses.
const ENGINE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);

/// How often a cancel reads the log again while it waits for the engine to
/// register, and to record its terminal after the SIGINT.
const ENGINE_SHUTDOWN_POLL: Duration = Duration::from_millis(200);

/// A run closed for good: the word its log has it stand in after the close,
/// and who closed it.
#[derive(Debug, Clone, PartialEq)]
pub struct ClosedRun {
    pub standing: Standing,
    pub by: Responder,
}

impl OpenedRun<'_> {
    /// Cancels this run, whatever its log says it is doing, and answers how
    /// the cancel ended. A run that moves has its engine interrupted and
    /// waited on, killed past the shutdown timeout, or, when the engine is
    /// gone, what it left settled and its pause recorded. A run that stopped
    /// has the process groups its engine could not stop killed. The caller's
    /// stop ends a wait on the engine, which keeps the interrupt.
    pub async fn cancel(&self, remarks: &dyn Remarks, stop: &Stop) -> Result<CancelState, Error> {
        let state = RunState::replay(&self.events);
        // Where the log leaves the run now: a pause a later wake resumed
        // stopped nothing that is still stopped.
        let parked = matches!(state.run.phase(), RunPhaseRaw::Paused);
        let stopped = parked || matches!(state.run.phase(), RunPhaseRaw::Closed);
        if state.broken.is_none() && !stopped {
            return self.stop_engine(&state, remarks, stop).await;
        }
        let killed = self.settle_stopped(remarks, stop).await;
        let standing = self
            .frame_of(&state, self.project.workspace.clock.now())
            .standing;
        Ok(CancelState::AlreadyStopped {
            killed,
            parked,
            standing,
        })
    }

    /// Closes this run for good, stopped and waiting on a person, as
    /// `cancelled` by `by`, or by the workspace's user marked unverified, and
    /// answers the word the run stands in now and who closed it. Refused when an engine drives it, its log says
    /// it moves, or it is closed already. A run whose engine died is settled
    /// first, as a cancel settles one; its checkouts go back to the project's
    /// pool once it is closed.
    pub async fn close(
        &self,
        by: Option<&Responder>,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<ClosedRun, Error> {
        let workspace = &self.project.workspace;
        // Resolved before anything is written: a short declared secret
        // refuses the close here, and the run stays as it is.
        let redactor = yunta_engine::run_redactor(&self.manifest.doc, workspace.secrets.as_ref())?;
        let storage = self.project.storage().await?;
        let engine = self.settled_engine(&storage, &redactor, remarks).await?;
        let by = Responder::answering(by, workspace.env.user.as_deref());
        let clock = workspace.clock.as_ref();
        yunta_engine::close_run(
            &storage,
            &self.id,
            &self.dir,
            clock,
            &engine,
            by.clone(),
            &redactor,
        )
        .await
        .map_err(|error| self.unclosed(error))?;
        self.give_back(remarks, stop).await;
        // The word is the log's, read after the close: what every reader of
        // the run says of it from now on.
        let events = storage
            .events_for_run(self.id.clone())
            .await
            .map_err(Failure::new)?;
        let standing = self
            .frame_of(&RunState::replay(&events), clock.now())
            .standing;
        Ok(ClosedRun { standing, by })
    }

    /// Stops a run whose log says it is moving: signals the engine it runs
    /// under and waits for that engine to record its terminal, or, when the
    /// engine is gone, settles what it left behind.
    async fn stop_engine(
        &self,
        state: &RunState,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<CancelState, Error> {
        // Resolved before anything is signalled or written: a short declared
        // secret leaves the run as it is.
        let secrets = self.project.workspace.secrets.as_ref();
        let redactor = yunta_engine::run_redactor(&self.manifest.doc, secrets)?;
        let storage = self.project.storage().await?;
        let Some(registry) = self.engine_registry(&storage, state, stop).await? else {
            return Ok(CancelState::NothingRunning);
        };
        if registry.liveness(&SystemProbe) == Liveness::Alive {
            // Case 1: the engine handles the rest itself.
            return self
                .signal_live_engine(&storage, &registry, remarks, stop)
                .await;
        }
        // Case 2: the engine crashed; its leftovers are this cancel's to
        // clean.
        self.settle_crash(&storage, &registry, &redactor, remarks)
            .await?;
        Ok(CancelState::Crashed {
            engine_pid: registry.engine_pid,
            killed: registry.process_groups.len(),
        })
    }

    /// Signals the live engine with SIGINT and waits for it to stop the run.
    async fn signal_live_engine(
        &self,
        storage: &AsyncStorage,
        registry: &EngineProcessFile,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<CancelState, Error> {
        remarks.remark(Remark::SignallingEngine {
            run: self.id.clone(),
            engine: registry.engine_pid,
        });
        signal_process(registry.engine_pid, Signal::SIGINT).map_err(|error| {
            Failure::new(EngineUnreached::Unsignalled {
                run: self.id.clone(),
                said: describe(&error),
            })
        })?;
        self.await_engine(storage, registry, remarks, stop).await
    }

    /// Waits for the engine a SIGINT went to to record its terminal on the
    /// log. Once the shutdown timeout is past, its process groups are killed
    /// with SIGKILL, and the engine itself too.
    async fn await_engine(
        &self,
        storage: &AsyncStorage,
        registry: &EngineProcessFile,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<CancelState, Error> {
        let engine_pid = registry.engine_pid;
        let deadline = tokio::time::Instant::now() + ENGINE_SHUTDOWN_TIMEOUT;
        loop {
            // A caller who stops waiting stopped waiting: a cancel's whole
            // body is a wait, so it answers to its own stop like the run it
            // is stopping answers to the engine's.
            tokio::select! {
                () = Pace::every(ENGINE_SHUTDOWN_POLL).beat() => {}
                () = stop.work().cancelled() => {
                    remarks.remark(Remark::StoppedWaiting { run: self.id.clone() });
                    return Ok(CancelState::Interrupted { engine_pid });
                }
            }
            let events = storage
                .events_for_run(self.id.clone())
                .await
                .map_err(Failure::new)?;
            // The pause this signal asks for, not one a wake has already
            // resumed.
            let state = RunState::replay(&events);
            if !matches!(state.run.phase(), RunPhaseRaw::Open) {
                let closed = matches!(state.run.phase(), RunPhaseRaw::Closed);
                return Ok(CancelState::Cancelled { engine_pid, closed });
            }
            if tokio::time::Instant::now() >= deadline {
                remarks.remark(Remark::Escalating {
                    run: self.id.clone(),
                    waited: ENGINE_SHUTDOWN_TIMEOUT,
                });
                groups::kill(&self.id, &registry.process_groups, remarks).await;
                if let Err(error) = signal_process(engine_pid, Signal::SIGKILL) {
                    if !error.is_gone() {
                        remarks.remark(Remark::Unstopped {
                            said: describe(&error),
                        });
                    }
                }
                return Ok(CancelState::Escalated { engine_pid });
            }
        }
    }

    /// Kills the process groups a stopped run's engine could not stop, and
    /// gives back the checkouts they held. Each group its registry names is
    /// killed, unless it is no run's own. When any of them is alive, the
    /// checkouts go back to the pool, as a close gives them back; the run's
    /// own state is left as it is. Answers how many of the named groups were
    /// alive as it looked.
    async fn settle_stopped(&self, remarks: &dyn Remarks, stop: &Stop) -> usize {
        let killed = groups::kill_unstopped(&self.dir, &self.id, remarks).await;
        if killed > 0 {
            self.give_back(remarks, stop).await;
        }
        killed
    }

    /// What a run whose engine died left behind, settled: every process
    /// group the engine registered killed, the run's pause recorded, and the
    /// registry naming the dead engine removed. The `run_paused` is emitted
    /// through the engine, not built here, so no event is stamped with a
    /// clock of the API's own. `redactor` is the run's own, resolved before
    /// anything here runs.
    async fn settle_crash(
        &self,
        storage: &AsyncStorage,
        registry: &EngineProcessFile,
        redactor: &Redactor,
        remarks: &dyn Remarks,
    ) -> Result<(), Error> {
        groups::kill(&self.id, &registry.process_groups, remarks).await;
        yunta_engine::record_pause_after_crash(
            storage,
            &self.id,
            &PauseReason::CancelledAfterCrash,
            self.project.workspace.clock.as_ref(),
            redactor,
        )
        .await?;
        let registry = yunta_engine::registry_path(&self.dir);
        if let Err(error) = tokio::fs::remove_file(registry).await {
            if error.kind() != std::io::ErrorKind::NotFound {
                remarks.remark(Remark::RegistryKept {
                    said: error.to_string(),
                });
            }
        }
        Ok(())
    }

    /// What drives the run, as a close judges it. A dead engine whose
    /// registry reads is settled first, so what it left behind is recorded
    /// before the close is written, and the run reads as no registry names
    /// it.
    async fn settled_engine(
        &self,
        storage: &AsyncStorage,
        redactor: &Redactor,
        remarks: &dyn Remarks,
    ) -> Result<Engine, Error> {
        let engine = self.registry().engine();
        if engine != Engine::Dead {
            return Ok(engine);
        }
        let Registry::Read(registry) = yunta_engine::read_registry(&self.dir) else {
            return Ok(engine);
        };
        self.settle_crash(storage, &registry.doc, redactor, remarks)
            .await?;
        Ok(Engine::Unrecorded)
    }

    /// What a close the run would not take, or could not finish, is.
    fn unclosed(&self, error: CloseRunError) -> Error {
        match error {
            CloseRunError::Refused(refusal) => Refused::Close {
                run: self.id.clone(),
                refusal,
            }
            .into(),
            CloseRunError::Storage(error) => Failure::new(error).into(),
            CloseRunError::Export(detail) => Failure::new(LogUnexported(detail)).into(),
        }
    }

    /// Gives this run's checkouts back to the project's pool, as a close does
    /// for a closed run and a cancel does once a stopped run's groups are
    /// gone. A checkout that cannot be given back is said through `remarks`,
    /// and stays the run's until `gc` collects it.
    async fn give_back(&self, remarks: &dyn Remarks, stop: &Stop) {
        let project = self.project;
        let manifest = &self.manifest.doc;
        let state = RunState::replay(&self.events);
        let bound = state.run.checkout().map(Path::to_path_buf);
        let tree = project.run_tree(manifest, bound.as_deref(), &self.id, &project.workspace.dir);
        let pool = project.pool_of(manifest, &self.dir);
        let supervision = project.workspace.supervision(stop);
        let kept = |error: yunta_engine::WorktreeError| {
            remarks.remark(Remark::CheckoutKept {
                run: self.id.clone(),
                said: error.to_string(),
            })
        };
        let units =
            yunta_engine::release_unit_checkouts(&pool, &tree, &self.dir, &self.id, supervision);
        if let Err(error) = units.await {
            kept(error);
        }
        if manifest.isolation == Isolation::Worktree {
            let own = yunta_engine::release_run_checkout(&pool, &tree, &self.id, supervision);
            if let Err(error) = own.await {
                kept(error);
            }
        }
    }
}

/// Why a cancel could not reach the engine of a run its log says moves; each
/// says what goes on from here.
#[derive(Debug)]
enum EngineUnreached {
    /// The run's registry is there and this binary cannot read it: what
    /// reading it said.
    RegistryUnread { run: RunId, said: String },
    /// A node of the run is in progress and no registry names the engine
    /// running it.
    NoRegistry { run: RunId },
    /// The interrupt did not reach the live engine: what the system said.
    Unsignalled { run: RunId, said: String },
}

impl std::fmt::Display for EngineUnreached {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let said = match self {
            EngineUnreached::RegistryUnread { run, said } => {
                yunta_render::ending::registry_unread(run, said)
            }
            EngineUnreached::NoRegistry { run } => yunta_render::ending::no_registry(run),
            EngineUnreached::Unsignalled { run, said } => {
                yunta_render::ending::unsignalled(run, said)
            }
        };
        f.write_str(&said)
    }
}

/// Each sentence already says what it met, so none has a cause behind it.
impl std::error::Error for EngineUnreached {}

/// A run's close on its log whose events could not be written out beside it:
/// what writing them said.
#[derive(Debug)]
struct LogUnexported(String);

impl std::fmt::Display for LogUnexported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&yunta_render::ending::log_unexported(&self.0))
    }
}

/// The detail is the export's own words, its causes in them already.
impl std::error::Error for LogUnexported {}
