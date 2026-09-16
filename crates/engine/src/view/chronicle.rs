//! The run as a sequence of facts, derived from the log beside the frame
//! and in the same types.
//!
//! A frame says where each thing stands right now; a chronicle says what
//! happened, in order. Both are pure folds of the same log, and what a
//! frame says a node *is*, a moment says it *became* — the very
//! [`NodeState`](yunta_core::events::NodeState) either way, so no
//! surface has to invent a second vocabulary for the same fact.

use std::time::Duration;

use chrono::{DateTime, Utc};

use yunta_core::events::meta::EventMeta;
use yunta_core::events::{
    artifacts, children, findings, gates, node, run, scope, session, tasks, EventPayload,
    StoredEvent,
};
use yunta_core::{NodeId, Seq};

use crate::replay::RunState;

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
/// Pure, and monotone in the log: the chronicle of a prefix is a prefix
/// of the chronicle. It folds the same `apply` the ledgers fold, and
/// reads each moment against the state *at that event* — so the time a
/// node worked and the children it dragged with it are the ones it had
/// then, not the ones the run ended with.
pub fn chronicle(events: &[StoredEvent]) -> Vec<Moment> {
    let mut state = RunState::default();
    let opened = events.first().map(|event| event.timestamp);
    events
        .iter()
        .map(|event| {
            let before = state.nodes.clone();
            // Folded exactly as `derive` folds, break included: once the
            // log stops making sense nothing further is applied, so the
            // two derivations never disagree about a state. Every event
            // still earns its moment — a reader not told the log carries
            // more than the binary could fold is a reader misled.
            if state.broken.is_none() {
                if let Err(error) = state.apply(event) {
                    state.broken = Some(error.to_string());
                }
            }
            let meta = EventMeta::of(event);
            Moment {
                seq: event.seq,
                at: event.timestamp,
                elapsed: opened
                    .and_then(|first| (event.timestamp - first).to_std().ok())
                    .unwrap_or_default(),
                node: event.node_id.clone(),
                happening: happening(event, &meta, &before, &state),
            }
        })
        .collect()
}

/// What one event says happened, in its own domain's words.
fn happening(
    event: &StoredEvent,
    meta: &EventMeta<'_>,
    before: &yunta_core::events::NodeLedger,
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
