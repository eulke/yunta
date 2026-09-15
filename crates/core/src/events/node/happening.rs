//! What a node-level event says happened, read as a person reads it.

use std::time::Duration;

use crate::events::meta::EventMeta;
use crate::events::{ChildLedger, ChildLink, NodeEvent, NodeLedger, NodeState, Reroute};
use crate::events::{HookPhase, Phase, ResolvedRunner};
use crate::TaskId;

/// One thing that happened to a node.
#[derive(Debug, Clone, PartialEq)]
pub enum Happening {
    RunnerResolved(ResolvedRunner),
    /// The state this event moved the node into, as the ledger derives
    /// it — the very type a frame reports. What a frame says a node
    /// *is*, this says it *became*.
    ///
    /// `elapsed` is how long the attempt that just closed worked, and
    /// `children` are the child runs it bore: both as the state stood
    /// at this event, never as it stands at the end of the log.
    Reached {
        state: NodeState,
        elapsed: Option<Duration>,
        children: Vec<ChildLink>,
    },
    Rerouted(Reroute),
    HookRan {
        phase: HookPhase,
        exit_code: i32,
    },
    ContextAssembled,
    CriteriaChecked {
        task: TaskId,
        phase: Phase,
        checked: usize,
    },
    ScopeChecked {
        violations: usize,
    },
    BaselineCaptured,
}

impl Happening {
    /// What `event` did to the node it names, with the state and the
    /// children that node had *at this event*.
    ///
    /// Two readings of the node ledger, because the moment spans the
    /// event: the state a node reached is a fact of the ledger `after`
    /// the event, and how long the attempt worked is a fact of the one
    /// `before` it — the close is what ends the attempt, so the ledger
    /// that still has it open is the only one that can measure it. Both
    /// are the ledgers as they stood here, never at the end of the log,
    /// so a node that settles twice reads as two different moments.
    pub fn of(
        event: &NodeEvent,
        meta: &EventMeta<'_>,
        before: &NodeLedger,
        after: &NodeLedger,
        children: &ChildLedger,
    ) -> Self {
        match event {
            NodeEvent::RunnerResolved(p) => Happening::RunnerResolved(ResolvedRunner {
                runner: p.runner.clone(),
                chosen: p.chosen.clone(),
                discarded: p.discarded.clone(),
            }),
            NodeEvent::Started(_) | NodeEvent::Finished(_) | NodeEvent::Failed(_) => {
                Happening::Reached {
                    state: meta
                        .node
                        .and_then(|node| after.state(node))
                        .cloned()
                        .unwrap_or_else(|| said(event)),
                    elapsed: meta.node.and_then(|node| worked(before, node, meta)),
                    children: meta
                        .node
                        .map(|node| bore(children, node))
                        .unwrap_or_default(),
                }
            }
            NodeEvent::Rerouted(p) => Happening::Rerouted(Reroute::of(p, meta)),
            NodeEvent::HookExecuted(p) => Happening::HookRan {
                phase: p.phase,
                exit_code: p.exit_code,
            },
            NodeEvent::ContextAssembled(_) => Happening::ContextAssembled,
            NodeEvent::CriteriaChecked(p) => Happening::CriteriaChecked {
                task: p.task_id.clone(),
                phase: p.phase,
                checked: p.results.len(),
            },
            NodeEvent::ScopeChecked(p) => Happening::ScopeChecked {
                violations: p.violations.len(),
            },
            NodeEvent::BaselineCaptured(_) => Happening::BaselineCaptured,
        }
    }
}

/// What the event says the node became, for a log the ledger stopped
/// folding before this point. The event itself is the record either
/// way; the ledger only adds what the payload does not carry.
fn said(event: &NodeEvent) -> NodeState {
    match event {
        NodeEvent::Finished(p) => NodeState::Finished {
            outcome: p.outcome.clone(),
            tokens: p.tokens_used,
        },
        NodeEvent::Failed(p) => NodeState::Failed {
            failure: p.failure.clone(),
            tokens: p.tokens_used,
            retryable: p.retryable,
        },
        NodeEvent::Started(p) => NodeState::Running { attempt: p.attempt },
        NodeEvent::RunnerResolved(_)
        | NodeEvent::Rerouted(_)
        | NodeEvent::HookExecuted(_)
        | NodeEvent::ContextAssembled(_)
        | NodeEvent::CriteriaChecked(_)
        | NodeEvent::ScopeChecked(_)
        | NodeEvent::BaselineCaptured(_) => NodeState::Running { attempt: 1 },
    }
}

/// How long the attempt this event closed had been working, or `None`
/// for a node with no attempt open — one that is starting has worked no
/// time at all, and saying `0s` would read as a measurement.
fn worked(before: &NodeLedger, node: &crate::NodeId, meta: &EventMeta<'_>) -> Option<Duration> {
    let record = before.get(node)?;
    let (_, since) = record.open_since?;
    (meta.at - since).to_std().ok()
}

/// The child runs this node bore, as the log has them at this event.
fn bore(children: &ChildLedger, node: &crate::NodeId) -> Vec<ChildLink> {
    children
        .links()
        .iter()
        .filter(|link| link.node.as_ref() == Some(node))
        .cloned()
        .collect()
}
