//! The display-only observation boundary: the engine hands every event
//! it appends to whoever is drawing, in the moment it writes it, so a
//! live surface renders from what the writer already holds instead of
//! polling the log it wrote itself an instant earlier.
//!
//! Nothing here derives state and nothing here decides. The log stays
//! the truth and replay stays how state comes off it; what an observer
//! holds is the cache of a single invocation, discarded with it.
//!
//! **What reaches an observer, and what cannot.** An event reaches a
//! run's log through the engine's `RunLog` and nowhere else, so the
//! mirror sits there: a log built with an observer hands it every event
//! it appends, in the order the log carries them. The scheduler's own
//! events, a live session's audit trail, what an agent posts
//! mid-session and every document a session hands over all append
//! through that one door, and so every one of them is observed without
//! knowing an observer exists. Three sites stay outside, and nothing
//! is lost by any of them:
//!
//! - `create_run` writes the run's birth — `run_created`, what the run
//!   is born holding, the measurement of its lineage's root among it —
//!   before an execution context, and so an observer, exists at all; a
//!   caller that draws the run reads those events as its seed. The
//!   measurement a run takes itself is a step of its own execution and
//!   goes through that door like every other event.
//! - `resolve_gate` and `record_pause_after_crash` append to storage
//!   directly, and only ever to a run *this* process is not executing:
//!   a parked one another process left behind, or one whose engine
//!   already died. There is no live view of that run here to mirror
//!   into.
//!
//! `crates/engine/tests/observer.rs` holds that list to the log itself:
//! it compares what an observer recorded against the run's own events,
//! so a new append site that skips this boundary fails there.
//!
//! **What a session says is not an event.** The log keeps a note's digest
//! and never its words, so the words reach an observer by another door,
//! [`RunObserver::said`], from the run's voices (the engine's `said`): redacted,
//! right after the digest is appended, and the same frame a reader of the
//! run's socket gets in another process. The list above does not count
//! them, and nothing an observer derives from the log moves on them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::events::EventPayload;
use crate::{NodeId, RunId, Seq, SessionId, TaskId};

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
    /// the engine's `execute_run` returns, every frame is
    /// delivered, because each one ran inline before its append helper
    /// returned.
    ///
    /// Returns nothing, also on purpose. A queue with no room is this
    /// boundary's policy, not a failure: the frame copies an event the
    /// log already holds, so losing it costs a drawing surface one
    /// repaint and costs the run nothing. A `Result` would hand every
    /// call site a display error whose only correct handling is to
    /// ignore it. Contrast
    /// the engine's `SessionObserver::record`,
    /// which does return one: a lost audit event thins the trail replay
    /// and `status` read, so its storage cause travels back and fails
    /// the node.
    fn observe(&self, event: Observed<'_>);

    /// Takes one thing a session said, which no log holds. It reaches here
    /// through the run's voices and never through the log's mirror, and it
    /// is never evidence: it moves nothing a surface derives from the log.
    /// Synchronous and silent for the reasons [`observe`](RunObserver::observe)
    /// is. Default: an observer that draws no words ignores it.
    fn said(&self, _said: &Said) {}
}

/// One appended event, borrowed for the duration of the call.
///
/// The body is the [`EventPayload`] itself, not the
/// [`StoredEvent`](crate::events::StoredEvent) envelope a reader
/// gets back off the log. That envelope's body is
/// [`EventBody::Known`](crate::events::EventBody::Known) or
/// [`EventBody::Unknown`](crate::events::EventBody::Unknown), and
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

/// What one session said, as a run hands it to whoever listens: redacted
/// before it leaves the engine, never on disk and never evidence. Not the
/// note's digest, which is what the log keeps of the same words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Said {
    pub run_id: RunId,
    pub node_id: NodeId,
    pub session_id: SessionId,
    /// The task the session works, when a loop opened it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// The seq of the note's digest on the log: where this sits among the
    /// run's events.
    pub seq: Seq,
    /// When it was said, by the run's clock.
    pub at: DateTime<Utc>,
    pub text: String,
    /// Whether `text` was cut to the most one frame carries, which the
    /// engine's voices bound before they hand a frame on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cut: bool,
}
