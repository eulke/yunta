//! Where an event sits, for a fold that needs to know.
//!
//! A ledger folds what happened, and some of what it derives is about
//! the event rather than the payload: which position justified a
//! transition, when a node was last heard from, which node a run-level
//! fact was attributed to. That is the envelope, and this is the part of
//! it a fold reads.

use chrono::{DateTime, Utc};

use crate::ids::{NodeId, Seq};

/// One event's envelope, as a ledger's `apply` reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventMeta<'a> {
    pub seq: Seq,
    pub at: DateTime<Utc>,
    /// The node the event was written under; `None` for a run-level
    /// fact.
    pub node: Option<&'a NodeId>,
}

impl<'a> EventMeta<'a> {
    /// The envelope of `event`.
    pub fn of(event: &'a crate::events::StoredEvent) -> Self {
        EventMeta {
            seq: event.seq,
            at: event.timestamp,
            node: event.node_id.as_ref(),
        }
    }
}
