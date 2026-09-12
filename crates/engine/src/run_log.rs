//! The one way an event reaches a run's log.
//!
//! Appending an event is three things that must never drift apart: the
//! run it belongs to, the node it speaks for, and the timestamp read
//! from the run's own injected clock. They travel together here, so a
//! site that records something — the scheduler, a node's close, a
//! session's tool listener, a run's birth — hands over a payload and
//! nothing else.

use yunta_core::events::{EventDraft, EventPayload};
use yunta_core::{Clock, NodeId, RunId, Seq};
use yunta_storage::{AsyncStorage, StorageError};

/// A handle on one run's log: where to append, which run, and the clock
/// that stamps the append.
pub(crate) struct RunLog<'a> {
    storage: &'a AsyncStorage,
    run_id: &'a RunId,
    clock: &'a dyn Clock,
}

impl<'a> RunLog<'a> {
    pub(crate) fn new(storage: &'a AsyncStorage, run_id: &'a RunId, clock: &'a dyn Clock) -> Self {
        RunLog {
            storage,
            run_id,
            clock,
        }
    }

    /// Appends one event against `node` — `None` for a run-level fact —
    /// and answers with the position storage gave it.
    ///
    /// The timestamp is read here, before the hop to the blocking thread
    /// that writes it, so every event of a run is stamped by the run's
    /// own clock and a test's fixed clock reaches every emitter.
    pub(crate) async fn record(
        &self,
        node: Option<&NodeId>,
        payload: EventPayload,
    ) -> Result<Seq, StorageError> {
        let draft = EventDraft {
            run_id: self.run_id.clone(),
            node_id: node.cloned(),
            payload,
        };
        let at = self.clock.now();
        self.storage.append(draft, at).await
    }
}
