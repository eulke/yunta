//! The run as a sequence of facts, derived from the log beside the frame
//! and in the same types.
//!
//! A frame says where each thing stands right now; a chronicle says what
//! happened, in order. Both are pure folds of the same log, and what a
//! frame says a node *is*, a moment says it *became* — the very
//! [`NodeState`](yunta_core::events::NodeState) either way, so no
//! surface has to invent a second vocabulary for the same fact.
//!
//! A reader that follows a run takes each event into its state and its
//! chronicle at once, with [`RunState::chronicled`]: the one fold, so the
//! state it holds is the one [`RunState::replay`] derives, and the moments
//! it gathered read by read are the [`chronicle`] of the whole log.

use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::RunState;
use yunta_core::events::meta::EventMeta;
use yunta_core::events::{
    artifacts, children, findings, gates, node, run, scope, session, tasks, EventPayload,
    NodeRecord, StoredEvent,
};
use yunta_core::{NodeId, Seq};

/// One thing that happened to a run, placed: when, to which node, and
/// what.
#[derive(Debug, Clone, PartialEq)]
pub struct Moment {
    pub seq: Seq,
    pub at: DateTime<Utc>,
    /// The run's own clock at this point: from its first event to this
    /// one. A reader following a run reads elapsed time, never a wall
    /// clock they have to subtract in their head.
    pub elapsed: Duration,
    /// The node it concerns; `None` for a run-level moment.
    pub node: Option<NodeId>,
    pub happening: Happening,
}

/// What happened, read as a person reads it. One arm per event domain,
/// and each domain owns the reading of its own kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum Happening {
    Run(run::happening::Happening),
    Node(node::happening::Happening),
    Session(session::happening::Happening),
    Tasks(tasks::happening::Happening),
    Scope(scope::happening::Happening),
    Findings(findings::happening::Happening),
    Artifacts(artifacts::happening::Happening),
    Gates(gates::happening::Happening),
    Children(children::happening::Happening),
    /// A kind this binary does not know. Still a moment: a reader not
    /// told the log carries more than the binary reads is a reader
    /// misled.
    Unknown {
        kind: String,
    },
}

/// Reads the log as the sequence of moments a person met it as: one per
/// event, in the log's order, each placed by the run's own elapsed.
///
/// [`RunState::chronicled`] over the slice, from a fresh state. Pure, and
/// monotone in the log: the chronicle of a prefix is a prefix of the
/// chronicle.
pub fn chronicle(events: &[StoredEvent]) -> Vec<Moment> {
    let mut state = RunState::default();
    events
        .iter()
        .filter_map(|event| state.chronicled(event))
        .collect()
}

impl RunState {
    /// Takes one event as [`fold`](RunState::fold) takes it, and hands
    /// back the moment it is, read against the state *at that event* — so
    /// the time a node worked and the children it dragged with it are the
    /// ones it had then, not the ones the run ended with.
    ///
    /// `None` for an event at or below the head: it was taken already, so
    /// news handed twice add no moment. Past the event where the log
    /// broke, an event moves nothing but the head and still earns its
    /// moment — a reader not told the log carries more than the binary
    /// could fold is a reader misled.
    ///
    /// Of the state before the event, a moment reads only the record of
    /// the node a node event names, so that record alone is kept across
    /// the fold: a run read whole costs its events, never a copy of every
    /// node's record, calls and all, at each of them.
    pub fn chronicled(&mut self, event: &StoredEvent) -> Option<Moment> {
        if self.has_taken(event.seq) {
            return None;
        }
        let before = match event.payload() {
            Some(EventPayload::Node(_)) => event
                .node_id
                .as_ref()
                .and_then(|node| self.nodes.get(node))
                .cloned(),
            _ => None,
        };
        self.fold(event);
        let meta = EventMeta::of(event);
        Some(Moment {
            seq: event.seq,
            at: event.timestamp,
            // On the run's awake clock, read off the state this event
            // leaves: a moment after the host slept sits where the run's
            // own time put it, not an hour on.
            elapsed: self.elapsed(Some(event.timestamp)).unwrap_or_default(),
            node: event.node_id.clone(),
            happening: happening(event, &meta, before.as_ref(), self),
        })
    }
}

/// What one event says happened, in its own domain's words.
fn happening(
    event: &StoredEvent,
    meta: &EventMeta<'_>,
    before: Option<&NodeRecord>,
    after: &RunState,
) -> Happening {
    let Some(payload) = event.payload() else {
        return Happening::Unknown {
            kind: event.body.kind_name().to_string(),
        };
    };
    match payload {
        EventPayload::Run(e) => Happening::Run(e.into()),
        EventPayload::Node(e) => Happening::Node(node::happening::Happening::of(
            e,
            meta,
            before,
            &after.nodes,
            &after.children,
            after.run.suspensions(),
        )),
        EventPayload::Session(e) => Happening::Session(e.into()),
        EventPayload::Tasks(e) => Happening::Tasks(e.into()),
        EventPayload::Scope(e) => Happening::Scope(e.into()),
        EventPayload::Findings(e) => Happening::Findings(e.into()),
        EventPayload::Artifacts(e) => Happening::Artifacts(e.into()),
        EventPayload::Gates(e) => Happening::Gates(e.into()),
        EventPayload::Children(e) => Happening::Children(e.into()),
    }
}
