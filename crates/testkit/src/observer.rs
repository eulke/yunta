//! `RecordingObserver` — the double every test that asserts on the
//! engine's display boundary runs against.

use std::sync::{Arc, Mutex};

use yunta_core::events::EventPayload;
use yunta_core::{NodeId, RunId, Seq};
use yunta_engine::{Observed, RunObserver};

/// One frame as it arrived, owned: the engine lends an
/// [`Observed`] only for the duration of the call, and a test asserts
/// long after that borrow is gone.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Which run appended the event — the invocation's own run, one of
    /// its `kind: workflow` children, or a promotion successor.
    pub run_id: RunId,
    pub seq: Seq,
    pub node_id: Option<NodeId>,
    /// The event's persisted `kind` — what an assertion on the shape of
    /// a run's log reads.
    pub kind: &'static str,
    pub payload: EventPayload,
}

/// Records every frame the engine delivers, in delivery order.
///
/// Recording is complete the moment `execute_run` returns, because
/// [`RunObserver::observe`] runs inline on the engine's own task: a test
/// asserts on [`frames`](RecordingObserver::frames) as soon as the run
/// comes back, with nothing to wait for and no synchronization of its
/// own to get right.
///
/// Hand the same recorder to a whole invocation and it collects the
/// parent's frames, its children's and its successors' together, in the
/// order the engine wrote them; [`for_run`](RecordingObserver::for_run)
/// separates them again.
#[derive(Default)]
pub struct RecordingObserver {
    frames: Mutex<Vec<Frame>>,
}

impl RecordingObserver {
    /// A recorder ready to hand to `RunEnv.observer`, which takes an
    /// [`Arc`] because the run-tools host keeps a clone of it.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Every frame recorded so far, in delivery order.
    pub fn frames(&self) -> Vec<Frame> {
        self.locked().clone()
    }

    /// Every frame's `kind`, in delivery order — the shape assertion a
    /// test usually wants, without the payloads.
    pub fn kinds(&self) -> Vec<&'static str> {
        self.locked().iter().map(|frame| frame.kind).collect()
    }

    /// Only the frames `run_id` appended, in delivery order — one run of
    /// an invocation that also drove children or successors.
    pub fn for_run(&self, run_id: &RunId) -> Vec<Frame> {
        self.locked()
            .iter()
            .filter(|frame| &frame.run_id == run_id)
            .cloned()
            .collect()
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, Vec<Frame>> {
        self.frames.lock().expect("recorded frames")
    }
}

impl RunObserver for RecordingObserver {
    fn observe(&self, event: Observed<'_>) {
        self.locked().push(Frame {
            run_id: event.run_id.clone(),
            seq: event.seq,
            node_id: event.node_id.cloned(),
            kind: event.payload.kind_name(),
            payload: event.payload.clone(),
        });
    }
}
