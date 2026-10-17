//! Where a run as a whole stands, and what a parked one is waiting on.
//!
//! A run's phase is read off the state replay derived from its own log:
//! the close that ended it, the pause that stopped it, the resume or the
//! node start that moved it again. Everything here is a pure function of
//! that state.
//!
//! A parked run carries the pause its own `run_paused` recorded, so the
//! frame answers *what* a person is being asked for and not only *which*
//! node is asking. No surface goes back to the log for it.

use yunta_core::events::{Failure, LoggedWords, PauseReason, TerminalState};
use yunta_core::{ModeName, NodeId, Workflow};

use crate::replay::{NodeState, NodeWait, RunState};
use yunta_core::events::RunPhaseRaw;

/// Where the run as a whole stands, derived from the log alone.
#[derive(Debug, Clone, PartialEq)]
pub enum RunPhase {
    /// The log carries no start event.
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
    /// `run_finished: failed`, carrying the failure of the node still
    /// standing in `Failed` that failed last — `None` when the log closed
    /// the run as failed with no such node on it, since nothing here
    /// invents one.
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

/// What a waiting run is waiting on. Only `PartialEq`, because a
/// [`PauseReason`] is only `PartialEq`.
#[derive(Debug, Clone, PartialEq)]
pub enum WaitingOn {
    /// A node parked on a person: `external_ref` is the forge's handle
    /// for a published gate, `None` for an internal gate and for
    /// unanswered questions. With several nodes parked at once this
    /// names the first in the workflow's declaration order; every one of
    /// them is in [`RunFrame::nodes`](super::RunFrame::nodes) and
    /// counted in [`RunFrame::flow`](super::RunFrame::flow).
    ///
    /// `reason` is the pause the run's own `run_paused` recorded, so no
    /// surface opens the log a second time to learn what the node asked
    /// for. A node id says *which* node stopped; the pause says *what* it
    /// asked — which questions are still unanswered, which cap a budget
    /// hit, which review a forge is holding. It is `None` for a node
    /// parked while the run itself keeps moving, where there is no
    /// standing pause to carry: a resume or a close came after the last
    /// one.
    ///
    /// `run_paused` is a run-level event and carries no node id, so the
    /// pause is the run's standing pause rather than this node's own:
    /// with several nodes parked at once, the two name the same stop
    /// only because one pause stops the whole run.
    Node {
        node: NodeId,
        /// What the node waits on — one vocabulary for the wait in the
        /// run and in the node.
        on: NodeWait,
        reason: Option<PauseReason>,
    },
    /// The run itself paused, with the pause `run_paused` recorded — an
    /// exhausted budget, a cancellation, a gate nobody has answered.
    Run { reason: PauseReason },
}

/// The run's phase: a log that stopped making sense says so first, then
/// the run's own close, then what it is parked on, then movement.
pub(super) fn phase(workflow: &Workflow, state: &RunState) -> RunPhase {
    if let Some(diagnostic) = &state.broken {
        return RunPhase::Broken {
            diagnostic: diagnostic.clone(),
        };
    }
    // The run's own ledger answers where it stands; the node ledger
    // says which node it stands on, when one does.
    match state.run.phase() {
        RunPhaseRaw::Unborn => RunPhase::Created,
        RunPhaseRaw::Closed => match state.run.finished() {
            Some(finished) => closed(&finished.terminal, state),
            None => RunPhase::Created,
        },
        // A node parked on a person is the more precise answer, and it
        // quotes the same pause, so naming the node costs the prose
        // nothing.
        RunPhaseRaw::Paused => {
            let reason = state.run.paused().map(|(reason, _)| reason);
            RunPhase::Waiting {
                on: waiting_node(workflow, state, reason).unwrap_or_else(|| WaitingOn::Run {
                    // Replay sets the paused phase together with its pause,
                    // so this empty fallback is never read.
                    reason: reason.cloned().unwrap_or(PauseReason::Recorded {
                        sentence: LoggedWords::default(),
                    }),
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

/// How a closed run closed, with the evidence its state carries for it:
/// the failure still standing in a failed node, the mode a promotion
/// signal suggested.
fn closed(terminal: &TerminalState, state: &RunState) -> RunPhase {
    match terminal {
        TerminalState::Done => RunPhase::Finished,
        TerminalState::Cancelled => RunPhase::Cancelled,
        TerminalState::Failed => RunPhase::Failed {
            failure: state.last_failure(),
        },
        TerminalState::Promoted => RunPhase::Promoted {
            to: state.run.promotion().map(|promotion| promotion.to.clone()),
        },
    }
}

/// The first node parked on a person, in the workflow's own declaration
/// order, carrying the run's standing pause.
fn waiting_node(
    workflow: &Workflow,
    state: &RunState,
    reason: Option<&PauseReason>,
) -> Option<WaitingOn> {
    workflow
        .iter_nodes()
        .find_map(|node| match state.nodes.state(&node.id) {
            Some(NodeState::Waiting { on }) => Some(WaitingOn::Node {
                node: node.id.clone(),
                on: on.clone(),
                reason: reason.cloned(),
            }),
            _ => None,
        })
}
