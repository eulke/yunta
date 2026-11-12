//! See [`super`]. What the sessions of a watched run said last, a frame to each node.
//!
//! It is kept while the node works, and while it is yet to start: a follower hears a
//! session off the run's socket and reads the node's start off the log, and the words
//! can come first. It is let go once the node stops working, so a node that works
//! again says something new, and what another run said is never kept, so a child's
//! words stay with the child.

use std::collections::HashMap;

use yunta_core::port::Said;
use yunta_core::{NodeId, RunId};
use yunta_replay::frame::{NodeStanding, RunFrame};

/// What the sessions of a watched run said last, a frame to each node. Not the note's
/// digest the log keeps of the same words: words are not evidence, and the next thing
/// said replaces them.
#[derive(Debug, Clone, Default)]
pub struct Heard {
    said: HashMap<(RunId, NodeId), Said>,
}

impl Heard {
    /// Keeps `said` for its node's row, in place of what the node said before.
    pub fn hear(&mut self, said: Said) {
        self.said
            .insert((said.run_id.clone(), said.node_id.clone()), said);
    }

    /// Lets go what a node of `frame` that stopped working said, and what a run other
    /// than `run_id` said; what a node yet to start said stays until it works.
    pub fn kept_to(&mut self, frame: &RunFrame, run_id: &RunId) {
        let working = super::working(frame);
        self.said.retain(|(run, node), _| {
            run == run_id
                && (working.iter().any(|working| &working.id == node) || yet_to_start(frame, node))
        });
    }

    /// What `node` of `run_id` said last, where it is kept.
    pub fn of(&self, run_id: &RunId, node: &NodeId) -> Option<&Said> {
        self.said.get(&(run_id.clone(), node.clone()))
    }
}

/// Whether `frame` holds `node` and has not started it.
fn yet_to_start(frame: &RunFrame, node: &NodeId) -> bool {
    frame
        .nodes
        .iter()
        .any(|held| &held.id == node && matches!(held.state, NodeStanding::ToGo))
}
