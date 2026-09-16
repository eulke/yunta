//! What the painter is told, and by whom.
//!
//! Two things reach it. The engine hands every append to
//! [`RunObserver::observe`] inline, on its own task, while the run
//! waits: nothing here may make it wait, so a full queue drops the
//! frame instead, because the frame is a copy of an event the log
//! already holds durably. What a drop costs is one seq in the painter's
//! stream, and a seq that stays missing is what sends the painter back
//! to the log.
//!
//! The other is the run itself. A line it has to say to the person
//! watching — an interrupt on its way to the run's cancellation, and
//! anything else raised while the region is on the terminal — goes the
//! same way an event does, because it lands in the same place: above the
//! region rather than around it. [`Diagnostics`] is the door it goes out
//! through.
//!
//! Who may draw at all is settled beside this, in
//! [`turns`](super::turns): the queue carries what is drawn, and the
//! curtain says whose turn it is.

use std::sync::Arc;

use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use yunta_core::events::{EventBody, StoredEvent};
use yunta_engine::{Observed, RunObserver};

/// One thing the painter is told, in the order it was told.
pub(super) enum Beat {
    /// An event the engine appended, already at its position on the log.
    Event(Box<StoredEvent>),
    /// A line the run has to say to the person watching it, to be
    /// written above the region rather than around it.
    Note(String),
    /// Nothing more is coming: fold what is queued ahead of this, then
    /// take the region down.
    Close,
}

/// The sender half, as the engine sees it.
///
/// Handed to `RunEnv.observer`, cloned by every `RunCtx` under it, and
/// dropped with the invocation. It derives nothing and decides nothing:
/// it copies what it is given into the queue and returns.
pub(super) struct Feed {
    beats: Sender<Beat>,
}

impl Feed {
    /// A feed and the receiving end the painter owns.
    pub(super) fn open() -> (Arc<Self>, Receiver<Beat>) {
        let (beats, received) = channel(super::QUEUE_DEPTH);
        (Arc::new(Self { beats }), received)
    }

    /// Sends a beat the surface itself raises, waiting for room rather
    /// than dropping it: neither a close nor a diagnostic is a repaint
    /// the queue can afford to miss.
    ///
    /// Answers whether the painter took it. A painter that has already
    /// stopped takes nothing — which is the state a close was asking
    /// for, and which leaves a caller holding a line a person has to
    /// read to say it somewhere else.
    pub(super) async fn tell(&self, beat: Beat) -> bool {
        self.beats.send(beat).await.is_ok()
    }

    /// Whether the painter has taken every beat sent so far off the
    /// queue — where a test that asserts on what the painter did with a
    /// beat waits for it to have done it.
    #[cfg(test)]
    pub(super) fn taken(&self) -> bool {
        self.beats.capacity() == self.beats.max_capacity()
    }
}

impl RunObserver for Feed {
    fn observe(&self, event: Observed<'_>) {
        match self.beats.try_send(Beat::Event(Box::new(stored(event)))) {
            Ok(()) => {}
            // The queue's policy, not a failure: the frame copies an
            // event the log already holds, so the painter recovers it by
            // reading the log rather than by this call waiting here.
            Err(TrySendError::Full(_) | TrySendError::Closed(_)) => {}
        }
    }
}

/// The observed event in the shape a reader gets off the log, which is
/// what every derivation takes.
///
/// The body is always [`EventBody::Known`]: the event was written by this
/// binary a moment ago, so there is no newer writer's kind to keep
/// verbatim.
fn stored(event: Observed<'_>) -> StoredEvent {
    StoredEvent {
        run_id: event.run_id.clone(),
        seq: event.seq,
        timestamp: event.at,
        node_id: event.node_id.cloned(),
        body: EventBody::Known(event.payload.clone()),
    }
}

/// Where a diagnostic raised while a run is being drawn goes out.
///
/// Nothing writes past the region except through it: a line printed
/// around it lands inside the rows it is redrawing and the next redraw
/// erases the copy it left there, so the person who just asked for that
/// line reads it for under a second. Handed to the painter instead, it
/// goes out above the region — and is held, like the lines the painter
/// already owes, for as long as a prompt has the terminal.
///
/// A diagnostic raised where nothing is drawing goes straight to
/// stderr, so a command with no surface, and a run whose surface has
/// already come down, say it exactly as they always did.
#[derive(Clone)]
pub(crate) struct Diagnostics(Option<Arc<Feed>>);

impl Diagnostics {
    /// The door onto `feed`'s painter.
    pub(super) fn through(feed: &Arc<Feed>) -> Self {
        Self(Some(Arc::clone(feed)))
    }

    /// A door with no surface behind it.
    pub(crate) fn none() -> Self {
        Self(None)
    }

    /// Says `diagnostic` to the person watching this run, above
    /// whatever is drawn.
    pub(crate) async fn raise(&self, diagnostic: impl std::fmt::Display) {
        let line = diagnostic.to_string();
        let taken = match &self.0 {
            Some(feed) => feed.tell(Beat::Note(line.clone())).await,
            None => false,
        };
        if !taken {
            crate::error::note(line);
        }
    }
}
