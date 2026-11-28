//! What the clock adds to a run's frame at one instant: how long the run has gone and where
//! its time went, how long it has stood since its last event, how long the process driving it
//! has asked a person at its terminal, and each node's times.
//!
//! A [`RunFrame`](super::RunFrame) says what the log says at its head and moves only when the
//! head does, or when the reading of the process driving the run does; everything here moves
//! with the clock between two events. So a reader that keeps a run's frame keeps it whole
//! across instants, and works these out at each instant from the state the frame was derived
//! from: a handful of reads per node, never a pass over the log nor over a node's calls.
//! Nothing here is kept, so nothing here goes stale.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_core::NodeId;

use crate::clock::TimeSpent;
use crate::registry::{Asking, Engine};
use crate::stats::{clock_read_to, open_attempt};
use crate::RunState;

/// What the clock adds to a run's frame at one instant, the instant it was worked out at:
/// every duration of the run measured there. Not [`RunFrame`](super::RunFrame), what the run's
/// log says at its head, which no instant moves.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunTimes {
    /// The run's wall-clock: to the instant while it has not finished, to its last event once
    /// it has. `None` for an empty log.
    pub elapsed: Option<Duration>,
    /// Where the run's wall-clock went: to the instant while the run is open, to its last
    /// event once it has ended.
    pub time: TimeSpent,
    /// From the log's last event to the instant: how long the run has stood where it is.
    /// Zero for an event stamped after the instant, `None` for a log with none.
    pub since_last_event: Option<Duration>,
    /// The person the process driving the run is asking at its terminal, measured at the
    /// instant. A question asked live is on no log until it is answered, so this is the one
    /// place a surface finds it.
    pub asking: Option<Asking>,
    /// Each node's times, by id. A node absent never started, wrote no event and kept no
    /// person waiting: [`RunTimes::node`] reads it as such.
    pub nodes: BTreeMap<NodeId, NodeTimes>,
}

/// What the clock adds to one node of a run's frame at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NodeTimes {
    /// How long this node has spent working: every stretch of work that closed, plus the one
    /// still open at the instant the run's clock reads: the instant while the run is going,
    /// its last applied event once it has finished or broken. Time spent waiting on a person
    /// is not work and is left out. `None` for a node that never started.
    pub elapsed: Option<Duration>,
    /// How long ago this node wrote its last event: measured, it grows while the node says
    /// nothing. `None` for a node with no events.
    pub last_event_age: Option<Duration>,
    /// How long a person was asked about this node, booked as the run's clock books a
    /// person's time, the stretch open at the instant included. Zero for a node no person
    /// was asked about.
    pub people: Duration,
}

impl RunTimes {
    /// What the clock adds at `now` to the frame of the run `state` derives, beside the
    /// process `engine` says drives it: `now` is the caller's own instant, as a frame's
    /// inputs are, so a reading at a chosen instant is the same in a test as in production.
    pub fn of(state: &RunState, engine: &Engine, now: DateTime<Utc>) -> RunTimes {
        let until = clock_read_to(state, Some(now));
        let asleep = state.run.suspensions();
        let mut nodes: BTreeMap<NodeId, NodeTimes> = state
            .nodes
            .iter()
            .map(|(id, record)| {
                let worked = record.first_started.map(|_| {
                    record.active + open_attempt(record, asleep, until).unwrap_or_default()
                });
                let times = NodeTimes {
                    elapsed: worked,
                    last_event_age: state.last_event_age(id, now),
                    people: Duration::ZERO,
                };
                (id.clone(), times)
            })
            .collect();
        for (node, people) in state.people_at(until) {
            nodes.entry(node).or_default().people = people;
        }
        RunTimes {
            elapsed: state.elapsed(until),
            time: state.time_spent(until),
            since_last_event: state.clock().since_last(now),
            asking: engine.asking(now),
            nodes,
        }
    }

    /// The times of `node`: none for a node that never started, wrote no event and kept no
    /// person waiting.
    pub fn node(&self, node: &NodeId) -> NodeTimes {
        self.nodes.get(node).copied().unwrap_or_default()
    }
}
