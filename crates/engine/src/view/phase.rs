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

use yunta_core::events::{EventPayload, Failure, StoredEvent, TerminalState};
use yunta_core::{ModeName, NodeId, Workflow};

use crate::replay::{NodeState, RunState};

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
pub(super) fn phase(workflow: &Workflow, state: &RunState, events: &[StoredEvent]) -> RunPhase {
    if let Some(diagnostic) = &state.broken {
        return RunPhase::Broken {
            diagnostic: diagnostic.clone(),
        };
    }
    let standing = standing(events);
    match standing.last {
        Some(EventPayload::RunFinished(p)) => closed(&p.terminal_state, events),
        // A node parked on a person is the more precise answer, and it
        // quotes the same pause, so naming the node costs the prose
        // nothing.
        Some(EventPayload::RunPaused(p)) => RunPhase::Waiting {
            on: waiting_node(workflow, state, standing.pause).unwrap_or_else(|| WaitingOn::Run {
                reason: p.reason.clone(),
            }),
        },
        Some(EventPayload::RunResumed(_) | EventPayload::NodeStarted(_)) => {
            match waiting_node(workflow, state, standing.pause) {
                Some(on) => RunPhase::Waiting { on },
                None => RunPhase::Running,
            }
        }
        _ => RunPhase::Created,
    }
}

/// What one walk back from the end of the log says about where a run
/// stands.
#[derive(Default)]
struct Standing<'a> {
    /// The event that last moved the run between phases: its close, its
    /// pause, its resume, or a node starting.
    last: Option<&'a EventPayload>,
    /// The reason the run's own `run_paused` recorded, while that pause
    /// still stands; `None` once a resume or a close left it behind.
    pause: Option<&'a str>,
}

/// Reads both of [`Standing`]'s answers in one pass, because they are
/// read off the same tail: a pause a run left behind is exactly a pause
/// with a resume or a close after it, and a node that started since
/// changes which phase the run is in without settling the pause it
/// quotes.
fn standing(events: &[StoredEvent]) -> Standing<'_> {
    let mut standing = Standing::default();
    for event in events.iter().rev() {
        let Some(payload) = event.payload() else {
            continue;
        };
        let settles = match payload {
            EventPayload::RunPaused(p) => {
                standing.pause = Some(p.reason.as_str());
                true
            }
            EventPayload::RunResumed(_) | EventPayload::RunFinished(_) => true,
            EventPayload::NodeStarted(_) => false,
            _ => continue,
        };
        if standing.last.is_none() {
            standing.last = Some(payload);
        }
        if settles {
            return standing;
        }
    }
    standing
}

/// How a closed run closed, with the evidence its own log carries for
/// it: the last `node_failed`'s failure, the last `promotion_signaled`'s
/// suggested mode.
fn closed(terminal: &TerminalState, events: &[StoredEvent]) -> RunPhase {
    match terminal {
        TerminalState::Done => RunPhase::Finished,
        TerminalState::Cancelled => RunPhase::Cancelled,
        TerminalState::Failed => RunPhase::Failed {
            failure: events.iter().rev().find_map(|event| match event.payload() {
                Some(EventPayload::NodeFailed(p)) => Some(p.failure.clone()),
                _ => None,
            }),
        },
        TerminalState::Promoted => RunPhase::Promoted {
            to: events.iter().rev().find_map(|event| match event.payload() {
                Some(EventPayload::PromotionSignaled(p)) => Some(p.suggested_mode.clone()),
                _ => None,
            }),
        },
    }
}

/// The first node parked on a person, in the workflow's own declaration
/// order, carrying the run's standing pause.
fn waiting_node(workflow: &Workflow, state: &RunState, reason: Option<&str>) -> Option<WaitingOn> {
    workflow
        .iter_nodes()
        .find_map(|node| match state.nodes.get(&node.id) {
            Some(NodeState::Waiting { external_ref }) => Some(WaitingOn::Node {
                node: node.id.clone(),
                external_ref: external_ref.clone(),
                reason: reason.map(str::to_string),
            }),
            _ => None,
        })
}
