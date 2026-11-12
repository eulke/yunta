//! See [`super`]. Which engine a cancel signals: the one the run's
//! `engine.json` names. A run its creator has just handed to an engine is
//! moving before that engine registers, so a cancel that finds no registry
//! waits for one, as it waits for the pause after its signal, instead of
//! finding nothing to stop.

use std::time::Duration;

use yunta_core::describe;
use yunta_core::events::{NodeState, RunPhaseRaw};
use yunta_core::pace::Pace;
use yunta_engine::{EngineProcessFile, Registry};
use yunta_replay::RunState;
use yunta_storage::AsyncStorage;

use super::{EngineUnreached, ENGINE_SHUTDOWN_POLL};
use crate::{Error, Failure, OpenedRun, Stop};

/// How long a cancel waits for a moving run's engine to register: an engine
/// registers once it has probed the adapters its run reaches, moments after
/// the run is handed to it, so one that has not by then is not coming.
const REGISTRATION_WAIT: Duration = Duration::from_secs(15);

impl OpenedRun<'_> {
    /// The engine registry a cancel signals through. A run whose log says it
    /// is moving and that no engine has registered yet is waited on until one
    /// registers. `None` when none does before the wait ends, the run stops
    /// on its own, or the caller's stop ends the wait; a failure when the
    /// registry does not read, or a node is in progress with no registry to
    /// reach it through.
    pub(super) async fn engine_registry(
        &self,
        storage: &AsyncStorage,
        state: &RunState,
        stop: &Stop,
    ) -> Result<Option<EngineProcessFile>, Error> {
        if let Some(registry) = self.registered(state)? {
            return Ok(Some(registry));
        }
        let deadline = tokio::time::Instant::now() + REGISTRATION_WAIT;
        while tokio::time::Instant::now() < deadline {
            tokio::select! {
                () = Pace::every(ENGINE_SHUTDOWN_POLL).beat() => {}
                () = stop.work().cancelled() => return Ok(None),
            }
            let events = storage
                .events_for_run(self.id.clone())
                .await
                .map_err(Failure::new)?;
            let state = RunState::replay(&events);
            if !matches!(state.run.phase(), RunPhaseRaw::Open) {
                return Ok(None);
            }
            if let Some(registry) = self.registered(&state)? {
                return Ok(Some(registry));
            }
        }
        Ok(None)
    }

    /// The registry the run's engine wrote, if it has written one. Without
    /// one, a node in progress has no channel to reach it, which is a
    /// failure; with no node in progress, the engine has not registered yet.
    fn registered(&self, state: &RunState) -> Result<Option<EngineProcessFile>, Error> {
        let unreached = match yunta_engine::read_registry(&self.dir) {
            Registry::Read(registry) => return Ok(Some(registry.doc)),
            // A registry that will not read is not a registry that was never
            // written: the reader is told which of the two it is, because
            // only one of them is a broken file on their disk.
            Registry::Corrupt(error) => EngineUnreached::RegistryUnread {
                run: self.id.clone(),
                said: describe(&error),
            },
            Registry::Absent => {
                let has_live_node = state
                    .nodes
                    .values()
                    .any(|record| matches!(record.state, Some(NodeState::Running { .. })));
                if !has_live_node {
                    return Ok(None);
                }
                EngineUnreached::NoRegistry {
                    run: self.id.clone(),
                }
            }
        };
        Err(Failure::new(unreached).into())
    }
}
