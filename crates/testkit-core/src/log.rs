//! A log written by hand, for a test that reads one.
//!
//! Ten test files each had a `fn event(seq, node, payload)` of their own
//! — the same four fields, spelled ten ways, each free to disagree with
//! the others about what a run id or a timestamp looks like. They agree
//! here instead: one builder, one run, one clock that moves when the
//! test says so, and positions from 1 in the order the events were
//! stated.

use chrono::{DateTime, Utc};

use yunta_core::events::{EventBody, EventPayload, StoredEvent};
use yunta_core::{NodeId, RunId};

use crate::clock::fixed_now;

/// One run's log, stated event by event.
///
/// ```ignore
/// let events = Log::for_run("run-1")
///     .node("lint", started(1))
///     .after(2)
///     .node("lint", failed("exit 1"))
///     .build();
/// ```
pub struct Log {
    run_id: RunId,
    at: DateTime<Utc>,
    events: Vec<StoredEvent>,
}

impl Log {
    /// An empty log for `id`, its clock at [`fixed_now`] — constant, so
    /// no assertion ever races the wall clock.
    pub fn for_run(id: &str) -> Self {
        Log {
            run_id: RunId::from(id),
            at: fixed_now(),
            events: Vec::new(),
        }
    }

    /// Moves the builder's clock to `instant`. Every event stated after
    /// this carries it, until the clock moves again.
    pub fn at(mut self, instant: DateTime<Utc>) -> Self {
        self.at = instant;
        self
    }

    /// Moves the builder's clock `secs` forward — how a test says time
    /// passed between two events without naming an instant.
    pub fn after(mut self, secs: i64) -> Self {
        self.at += chrono::Duration::seconds(secs);
        self
    }

    /// States one run-level event: a fact with no node behind it.
    pub fn event(self, payload: EventPayload) -> Self {
        self.push(None, payload)
    }

    /// States one event attributed to `node`.
    pub fn node(self, node: &str, payload: EventPayload) -> Self {
        self.push(Some(NodeId::from(node)), payload)
    }

    /// The log as a reader meets it: positions from 1, in the order the
    /// events were stated.
    pub fn build(self) -> Vec<StoredEvent> {
        self.events
    }

    fn push(mut self, node: Option<NodeId>, payload: EventPayload) -> Self {
        self.events.push(StoredEvent {
            run_id: self.run_id.clone(),
            seq: ((self.events.len() + 1) as u64).into(),
            timestamp: self.at,
            node_id: node,
            body: EventBody::Known(payload),
        });
        self
    }
}
