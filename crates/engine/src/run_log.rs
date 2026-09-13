//! The one way an event reaches a run's log.
//!
//! Appending an event is three things that must never drift apart: the
//! run it belongs to, the node it speaks for, and the timestamp read
//! from the run's own injected clock. They travel together here, so a
//! site that records something — the scheduler, a node's close, a
//! session's tool listener, a run's birth — hands over a payload and
//! nothing else.

use yunta_core::events::{EventDraft, EventPayload, StoredEvent};

use crate::observer::{Observed, RunObserver};
use yunta_core::{Clock, NodeId, RunId, Seq};
use yunta_storage::{AsyncStorage, StorageError};

/// A handle on one run's log: where to append, which run, and the clock
/// that stamps the append.
pub(crate) struct RunLog<'a> {
    storage: &'a AsyncStorage,
    run_id: &'a RunId,
    clock: &'a dyn Clock,
    observer: Option<&'a dyn RunObserver>,
}

impl<'a> RunLog<'a> {
    pub(crate) fn new(storage: &'a AsyncStorage, run_id: &'a RunId, clock: &'a dyn Clock) -> Self {
        RunLog {
            storage,
            run_id,
            clock,
            observer: None,
        }
    }

    /// The same log, mirroring every append it makes to `observer`.
    ///
    /// The mirror hangs here because this is the one way an event
    /// reaches the log: a site that appends through a log built this way
    /// feeds a live view by doing nothing about it. What is left
    /// unmirrored is exactly what is built without this — `run_created`,
    /// written before an execution context, and so an observer, exists.
    pub(crate) fn observed_by(mut self, observer: Option<&'a dyn RunObserver>) -> Self {
        self.observer = observer;
        self
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
        let Some(observer) = self.observer else {
            return self.storage.append(draft, at).await;
        };
        // The mirror costs one clone, because `append` takes the draft
        // by value — paid only while something is watching, which is
        // what makes the `Option` load-bearing rather than nullability
        // sugar. The frame goes out once storage has accepted the event
        // and named its seq: a frame for an event that was never written
        // would be a view of something that did not happen.
        let mirrored = draft.clone();
        let seq = self.storage.append(draft, at).await?;
        observer.observe(Observed {
            run_id: self.run_id,
            seq,
            at,
            node_id: mirrored.node_id.as_ref(),
            payload: &mirrored.payload,
        });
        Ok(seq)
    }

    /// Every event this run's log holds, in order.
    ///
    /// The one read of a run's own log: a site deriving state, looking
    /// for what a node produced or asking what it already registered
    /// goes through here, so the log has a single reader the way it has
    /// a single writer.
    pub(crate) async fn events(&self) -> Result<Vec<StoredEvent>, StorageError> {
        self.storage.events_for_run(self.run_id.clone()).await
    }
}
