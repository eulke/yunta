//! What a failed node does next: it is re-routed to a correction, paused
//! on its menu, or left for a person. Its facts come from the log, and the
//! menu it offers is [`Failure::remedies`].

use chrono::{DateTime, Utc};
use yunta_core::events::{Facts, Failure, Outlook, PauseReason, Stuck};
use yunta_core::{DefaultOnFailure, Node};

use super::{after_reroute, is_gate, Board, Decision};
use crate::replay::NodeState;

impl Board<'_> {
    /// What the log holds beside `failure` of `node`: the clock it failed
    /// at, the node's tasks, whether a person may widen its scope, and
    /// whether the run's tree moved since. Read from the log alone.
    fn facts_of(&self, node: &Node, failure: &Failure, failed_at: DateTime<Utc>) -> Facts {
        Facts {
            failed_at,
            continuable: self.state.tasks.continuable_by(&node.id),
            may_grant: self.grantable(node, failure),
            tree_moved: self.state.tree_moved_since_failure(&node.id),
        }
    }
}

/// Failures, one at a time: re-route, hand control to a pending
/// corrective node, return control to a corrected node, or stop. A
/// failure this call leaves unresolved is picked up again on the next
/// (the reroute or restart it emits changes the log, so the next call
/// sees a different answer for it).
pub(super) fn failure_step(board: &Board<'_>) -> Option<Decision> {
    for node in board.nodes.iter().copied() {
        let Some(NodeState::Failed { failure, .. }) = board.state.nodes.state(&node.id) else {
            continue;
        };
        let record = board.state.nodes.get(&node.id).cloned().unwrap_or_default();
        let rerouted_for_this_failure = record
            .last_reroute
            .as_ref()
            .filter(|reroute| Some(reroute.seq) > record.last_failed)
            .map(|reroute| (reroute.seq, reroute.to.clone()));

        match rerouted_for_this_failure {
            None => {
                let failed_at = record.last_terminal_at.unwrap_or(DateTime::UNIX_EPOCH);
                if let Some(decision) =
                    unrerouted(board, node, failure, record.on_failure_reroutes, failed_at)
                {
                    return Some(decision);
                }
            }
            Some((reroute_seq, to)) => {
                if let Some(decision) = after_reroute(board, node, &to, reroute_seq) {
                    return Some(decision);
                }
            }
        }
    }
    None
}

/// A failure with no re-route after it: its own `on_failure` re-routes
/// or escalates, and a node without one leaves the decision to the
/// policy. `continue` is the one answer that is not a decision — the
/// node stays failed, its dependents stay unscheduled, and the rest of
/// the graph keeps running. Under `pause` a person decides: a wake past a
/// limit's reset runs the node again, and so does a `retry` they chose
/// after this failure while the menu still offers it. A gate node keeps
/// the plain pause, since its own flow is what re-asks it.
///
/// A correction that cannot change the failure is not offered: a limit
/// that resets is paused for, and a correction would meet the same limit;
/// a correction reads the same frozen config the node failed on, so
/// re-routing to it spends a node on an outcome already known.
fn unrerouted(
    board: &Board<'_>,
    node: &Node,
    failure: &Failure,
    reroutes: u32,
    failed_at: DateTime<Utc>,
) -> Option<Decision> {
    let outlook = failure.remedies(&board.facts_of(node, failure, failed_at));
    let correctable = node
        .on_failure
        .as_ref()
        .filter(|_| correction_can_change(failure, &outlook));
    if let Some(on_failure) = correctable {
        if reroutes < on_failure.max_reroutes {
            return Some(Decision::Reroute {
                from: node.id.clone(),
                to: on_failure.goto.clone(),
                attempt: reroutes + 1,
                max_reroutes: on_failure.max_reroutes,
                cause: failure.clone(),
            });
        }
        return Some(Decision::GateExhaustedReroutes {
            node: node.id.clone(),
            goto: on_failure.goto.clone(),
            max_reroutes: on_failure.max_reroutes,
            cause: failure.clone(),
        });
    }
    match board.policy.on_failure {
        DefaultOnFailure::Pause => Some(paused_failure(board, node, failure, outlook)),
        DefaultOnFailure::Abort => Some(Decision::Fail {
            node: node.id.clone(),
            failure: failure.clone(),
        }),
        DefaultOnFailure::Continue => None,
    }
}

/// What a failed node does under `on_failure: pause`: it pauses until its
/// limit resets and runs again once a wake finds the reset past, runs again
/// on a `retry` a person chose that the menu still offers, or asks a person
/// what to do. A gate node keeps the plain pause.
fn paused_failure(board: &Board<'_>, node: &Node, failure: &Failure, outlook: Outlook) -> Decision {
    let next_attempt = board.next_attempt(&node.id);
    match outlook {
        Outlook::WaitUntil { at } if board.state.reset_waited(&node.id, at) => {
            Decision::Execute(vec![(node.id.clone(), next_attempt)])
        }
        Outlook::WaitUntil { at } => Decision::WaitUntil {
            node: node.id.clone(),
            at,
        },
        _ if is_gate(node) => Decision::Pause {
            reason: PauseReason::NodeFailed {
                node: node.id.clone(),
                failure: failure.clone(),
            },
        },
        _ if board.retry_chosen(&node.id, &outlook) => {
            Decision::Execute(vec![(node.id.clone(), next_attempt)])
        }
        outlook => Decision::EscalateFailure {
            node: node.id.clone(),
            failure: failure.clone(),
            next_attempt,
            outlook,
        },
    }
}

/// Whether a re-route can change how `failure` ends. A limit that resets
/// meets the same limit on the correction's attempt, a frozen config reads
/// the same to every attempt, and a plan that repeats the declaration its
/// correction already answered is the same plan.
fn correction_can_change(failure: &Failure, outlook: &Outlook) -> bool {
    let repeats_a_plan = matches!(
        failure,
        Failure::Unchanged { unchanged }
            if matches!(unchanged.failure.as_ref(), Failure::PlanUnpassable { .. })
    );
    let waits = matches!(outlook, Outlook::WaitUntil { .. });
    !repeats_a_plan && !waits && !matches!(outlook, Outlook::Stuck(Stuck::FrozenConfig))
}
