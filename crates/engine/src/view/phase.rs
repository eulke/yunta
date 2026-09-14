//! Where a run as a whole stands, and what a parked one is waiting on.
//!
//! A run's phase is read off the tail of its own log: the close that
//! ended it, the pause that stopped it, the resume or the node start
//! that moved it again. Everything here is a pure function of that log
//! and of the state replay derived from it.
//!
//! A parked run carries the sentence its own `run_paused` recorded, so
//! the frame answers *what* a person is being asked for and not only
//! *which* node is asking. No surface goes back to the log for it.

use yunta_core::events::{Failure, StoredEvent, TerminalState};
use yunta_core::{ModeName, NodeId, Workflow};

use crate::replay::{NodeState, RunState};
use yunta_core::events::RunPhaseRaw;

/// Where the run as a whole stands, derived from the log alone.
#[derive(Debug, Clone, PartialEq)]
pub enum RunPhase {
    /// The log carries no start yet.
    Created,
    Running,
    /// Parked on a person: at least one node is on a published gate or
    /// on unanswered questions, or the run itself paused for a decision.
    /// A run whose other nodes keep moving reports this too — what is
    /// running is in [`RunFrame::flow`](super::RunFrame::flow) beside
    /// what is parked, so the two are read together.
    Waiting {
        on: WaitingOn,
    },
    /// `run_finished: done`.
    Finished,
    /// `run_finished: failed`, carrying the failure of the last
    /// `node_failed` on the log — `None` when the log closed the run as
    /// failed with no node failure on it, since nothing here invents
    /// one.
    Failed {
        failure: Option<Failure>,
    },
    /// `run_finished: cancelled` — a run a person stopped, which is
    /// neither a failure nor a completion and is reported as itself.
    Cancelled,
    /// `run_finished: promoted` — this run closed for a successor in a
    /// later mode. `to` is the mode the `promotion_signaled` behind it
    /// suggested; `None` for a log that closed a run as promoted with no
    /// such event, where only the successor's own `run_created` names
    /// the mode.
    Promoted {
        to: Option<ModeName>,
    },
    /// The log stopped making sense at the event the diagnostic names;
    /// the rest of the frame is what replay derived before it.
    Broken {
        diagnostic: String,
    },
}

/// What a waiting run is waiting on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitingOn {
    /// A node parked on a person: `external_ref` is the forge's handle
    /// for a published gate, `None` for an internal gate and for
    /// unanswered questions. With several nodes parked at once this
    /// names the first in the workflow's declaration order; every one of
    /// them is in [`RunFrame::nodes`](super::RunFrame::nodes) and
    /// counted in [`RunFrame::flow`](super::RunFrame::flow).
    ///
    /// `reason` is the sentence the run's own `run_paused` recorded, so
    /// no surface opens the log a second time to learn what the node
    /// asked for. A node id says *which* node stopped; the sentence says
    /// *what* it asked — which questions are still unanswered, which cap
    /// a budget hit, which review a forge is holding. It is `None` for a
    /// node parked while the run itself keeps moving, where there is no
    /// standing pause to quote: a resume or a close came after the last
    /// one.
    ///
    /// `run_paused` is a run-level event and carries no node id, so the
    /// sentence is the run's standing pause rather than this node's own:
    /// with several nodes parked at once, the two name the same stop
    /// only because one pause stops the whole run.
    Node {
        node: NodeId,
        external_ref: Option<String>,
        reason: Option<String>,
    },
    /// The run itself paused, with the reason `run_paused` recorded — an
    /// exhausted budget, a cancellation, a gate nobody has answered.
    Run { reason: String },
}

/// The run's phase: a log that stopped making sense says so first, then
/// the run's own close, then what it is parked on, then movement.
pub(super) fn phase(workflow: &Workflow, state: &RunState, _events: &[StoredEvent]) -> RunPhase {
    if let Some(diagnostic) = &state.broken {
        return RunPhase::Broken {
            diagnostic: diagnostic.clone(),
        };
    }
    // The run's own ledger answers where it stands; the node ledger
    // says which node it stands on, when one does.
    match state.run.phase() {
        RunPhaseRaw::Unborn => RunPhase::Created,
        RunPhaseRaw::Closed => match state.run.closed() {
            Some((terminal, _)) => closed(terminal, state),
            None => RunPhase::Created,
        },
        // A node parked on a person is the more precise answer, and it
        // quotes the same pause, so naming the node costs the prose
        // nothing.
        RunPhaseRaw::Paused => {
            let reason = state.run.paused().map(|(reason, _)| reason);
            RunPhase::Waiting {
                on: waiting_node(workflow, state, reason).unwrap_or_else(|| WaitingOn::Run {
                    reason: reason.unwrap_or_default().to_string(),
                }),
            }
        }
        RunPhaseRaw::Open => match waiting_node(workflow, state, None) {
            Some(on) => RunPhase::Waiting { on },
            None if state.nodes.values().any(|record| record.state.is_some()) => RunPhase::Running,
            None => RunPhase::Created,
        },
    }
}

/// How a closed run closed, with the evidence its own log carries for
/// it: the last `node_failed`'s failure, the last `promotion_signaled`'s
/// suggested mode.
fn closed(terminal: &TerminalState, state: &RunState) -> RunPhase {
    match terminal {
        TerminalState::Done => RunPhase::Finished,
        TerminalState::Cancelled => RunPhase::Cancelled,
        TerminalState::Failed => RunPhase::Failed {
            failure: state
                .nodes
                .values()
                .filter_map(|record| match (&record.state, record.last_failed) {
                    (Some(NodeState::Failed { failure, .. }), Some(seq)) => Some((seq, failure)),
                    _ => None,
                })
                .max_by_key(|(seq, _)| *seq)
                .map(|(_, failure)| failure.clone()),
        },
        TerminalState::Promoted => RunPhase::Promoted {
            to: state.run.promotion().map(|promotion| promotion.to.clone()),
        },
    }
}

/// The first node parked on a person, in the workflow's own declaration
/// order, carrying the run's standing pause.
fn waiting_node(workflow: &Workflow, state: &RunState, reason: Option<&str>) -> Option<WaitingOn> {
    workflow
        .iter_nodes()
        .find_map(|node| match state.nodes.state(&node.id) {
            Some(NodeState::Waiting { external_ref }) => Some(WaitingOn::Node {
                node: node.id.clone(),
                external_ref: external_ref.clone(),
                reason: reason.map(str::to_string),
            }),
            _ => None,
        })
}
