//! Which engine a cancel signals: the one the run's `engine.json` names. A
//! run its creator has just handed to an engine is moving before that engine
//! registers, so a cancel that finds no registry waits for one, as it waits
//! for the pause after its signal, instead of finding nothing to stop.

use std::time::Duration;

use yunta_core::events::RunPhaseRaw;
use yunta_core::{describe, RunId};
use yunta_engine::pace::Pace;
use yunta_engine::{EngineProcessFile, NodeState, Registry, RunState};

use super::ENGINE_SHUTDOWN_POLL;
use crate::commands::advice;
use crate::context::{Context, Opened};
use crate::error::CliError;

/// How long a cancel waits for a moving run's engine to register: an engine
/// registers once it has probed the adapters its run reaches, moments after
/// the run is handed to it, so one that has not by then is not coming.
const REGISTRATION_WAIT: Duration = Duration::from_secs(15);

/// The engine registry a cancel signals through. A run whose log says it is
/// moving and that no engine has registered yet is waited on until one
/// registers. `None` when none does before the wait ends, the run stops on
/// its own, or the person stops waiting; an error when a node is in progress
/// with no channel to reach it.
pub(super) async fn engine_registry(
    ctx: &Context,
    storage: &yunta_storage::AsyncStorage,
    open: &Opened,
    run_id: &RunId,
    state: &RunState,
) -> Result<Option<EngineProcessFile>, CliError> {
    if let Some(registry) = registered(open, run_id, state)? {
        return Ok(Some(registry));
    }
    let deadline = tokio::time::Instant::now() + REGISTRATION_WAIT;
    while tokio::time::Instant::now() < deadline {
        tokio::select! {
            () = Pace::every(ENGINE_SHUTDOWN_POLL).beat() => {}
            () = ctx.cancellation().cancelled() => return Ok(None),
        }
        let state = yunta_engine::derive(&storage.events_for_run(run_id.clone()).await?);
        if !matches!(state.run.phase(), RunPhaseRaw::Open) {
            return Ok(None);
        }
        if let Some(registry) = registered(open, run_id, &state)? {
            return Ok(Some(registry));
        }
    }
    Ok(None)
}

/// The registry the run's engine wrote, if it has written one. Without one,
/// a node in progress has no channel to reach it, which is an error; with
/// no node in progress, the engine has not registered yet.
fn registered(
    open: &Opened,
    run_id: &RunId,
    state: &RunState,
) -> Result<Option<EngineProcessFile>, CliError> {
    let called = run_id.handle();
    match yunta_engine::read_registry(&open.run_dir) {
        Registry::Read(registry) => Ok(Some(registry.doc)),
        // A registry that will not read is not a registry that was
        // never written: the person is told which of the two it is,
        // because only one of them is a broken file on their disk.
        Registry::Corrupt(error) => Err(CliError::msg(format!(
            "run {called}: its `engine.json` is there and this binary cannot read it \
             ({}) — `{}` recovers the run once its process has stopped.",
            describe(&error),
            advice::resume(run_id.handle())
        ))),
        Registry::Absent => {
            let has_live_node = state
                .nodes
                .values()
                .any(|record| matches!(record.state, Some(NodeState::Running { .. })));
            match has_live_node {
                false => Ok(None),
                true => Err(CliError::msg(format!(
                    "run {called} has a node in progress but no `engine.json` to signal \
                     through — the engine that ran it predates this build, or its scratch \
                     directory is gone. `{}` recovers the run once its process has stopped.",
                    advice::resume(run_id.handle())
                ))),
            }
        }
    }
}
