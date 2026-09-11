//! The display-only observation boundary: the engine hands every event
//! it appends to whoever is drawing, in the moment it writes it, so a
//! live surface renders from what the writer already holds instead of
//! polling the log it wrote itself an instant earlier.
//!
//! Nothing here derives state and nothing here decides. The log stays
//! the truth and replay stays how state comes off it; what an observer
//! holds is the cache of a single invocation, discarded with it.
//!
//! **What reaches an observer, and what cannot.** Every append a run
//! makes while this process executes it goes through
//! [`append_observed`]: `RunCtx::emit` (the run's own events),
//! `RunCtx`'s [`SessionObserver`](crate::SessionObserver) impl (a live
//! session's audit events) and the run-tools listener's `post_finding`
//! (what an agent posts mid-session). Three appends stay outside, and
//! nothing is lost by any of them:
//!
//! - `create_run` writes `run_created` before a `RunCtx` — and so an
//!   observer — exists at all; a caller that draws the run reads that
//!   one event as its seed.
//! - `resolve_gate` and `record_pause_after_crash` only ever write to a
//!   run *this* process is not executing: a parked one another process
//!   left behind, or one whose engine already died. There is no live
//!   view of that run here to mirror into.
//!
//! `crates/engine/tests/observer.rs` holds that list to the log itself:
//! it compares what an observer recorded against the run's own events,
//! so a new append site that skips this boundary fails there.

use chrono::{DateTime, Utc};
use yunta_core::events::{EventDraft, EventPayload};
use yunta_core::{NodeId, RunId, Seq};
use yunta_storage::{AsyncStorage, StorageError};

/// A live view of a run's log, fed as the engine writes it.
///
/// Every implementation is infallible and non-blocking by contract:
/// [`observe`](RunObserver::observe) runs on the engine's own task,
/// inline inside the append helper, after storage assigned the seq and
/// before the caller's next step. It has no way to report a failure and
/// no permission to wait for one, so an implementation that cannot keep
/// up drops the frame, because the frame is a copy of something the log
/// already holds durably. The log is the truth; this is a mirror of it.
///
/// One observer serves a whole invocation: the run, every `kind:
/// workflow` child it gives birth to, and every promotion successor
/// driven after it. [`Observed::run_id`] names which.
pub trait RunObserver: Send + Sync {
    /// Takes one event that is already on the log.
    ///
    /// Synchronous on purpose. An `async fn` would put the emit path
    /// under the observer's own readiness — an `.await` inside the
    /// append helper is the exact stall this boundary exists to remove —
    /// while a plain `fn` is structurally incapable of making the run
    /// wait. It is also what makes shutdown race-free: by the time
    /// [`execute_run`](crate::execute_run) returns, every frame is
    /// delivered, because each one ran inline before its append helper
    /// returned.
    ///
    /// Returns nothing, also on purpose. A queue with no room is this
    /// boundary's policy, not a failure: the frame copies an event the
    /// log already holds, so losing it costs a drawing surface one
    /// repaint and costs the run nothing. A `Result` would hand every
    /// call site a display error whose only correct handling is to
    /// ignore it. Contrast
    /// [`SessionObserver::emit_session_event`](crate::SessionObserver::emit_session_event),
    /// which does return one: a lost audit event thins the trail replay
    /// and `status` read, so its storage cause travels back and fails
    /// the node.
    fn observe(&self, event: Observed<'_>);
}

/// One appended event, borrowed for the duration of the call.
///
/// The body is the [`EventPayload`] itself, not the
/// [`StoredEvent`](yunta_core::events::StoredEvent) envelope a reader
/// gets back off the log. That envelope's body is
/// [`EventBody::Known`](yunta_core::events::EventBody::Known) or
/// [`EventBody::Unknown`](yunta_core::events::EventBody::Unknown), and
/// the unknown arm exists for a reader parsing a *newer* writer's log —
/// a state this boundary cannot produce, because the event it carries
/// was written by this binary a moment ago. Handing over the envelope
/// would oblige every renderer to handle a case that never arrives.
#[derive(Debug, Clone, Copy)]
pub struct Observed<'a> {
    /// Which run appended the event: the invocation's own run, a `kind:
    /// workflow` child of it, or a promotion successor driven after it.
    pub run_id: &'a RunId,
    /// The position storage assigned — the event is durable at this
    /// position before the frame is handed over.
    pub seq: Seq,
    /// The run's injected clock, read once for this event and carried
    /// on the log's own row.
    pub at: DateTime<Utc>,
    /// The node the event concerns, or `None` for a run-level event.
    pub node_id: Option<&'a NodeId>,
    /// What happened, in the same shape the log row carries. Its
    /// `kind_name` is the discriminant a surface switches on.
    pub payload: &'a EventPayload,
}

/// Appends `draft` and mirrors it to `observer` — the one place the
/// display boundary is fed, so every observable append site feeds it
/// the same way.
///
/// The mirror costs one clone of the draft, because
/// [`AsyncStorage::append`] takes it by value, and that clone is paid
/// only while something is watching: with no observer the draft goes
/// straight through, which is what makes the `Option` load-bearing
/// rather than nullability sugar. The frame goes out only once storage
/// has accepted the event and named its seq — a frame for an event that
/// was never written would be a lie.
pub(crate) async fn append_observed(
    storage: &AsyncStorage,
    observer: Option<&dyn RunObserver>,
    draft: EventDraft,
    at: DateTime<Utc>,
) -> Result<Seq, StorageError> {
    let Some(observer) = observer else {
        return storage.append(draft, at).await;
    };
    let mirrored = draft.clone();
    let seq = storage.append(draft, at).await?;
    observer.observe(Observed {
        run_id: &mirrored.run_id,
        seq,
        at,
        node_id: mirrored.node_id.as_ref(),
        payload: &mirrored.payload,
    });
    Ok(seq)
}
