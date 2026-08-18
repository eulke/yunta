//! The scheduler's decision function (T4.1 recorte) — pure.
//!
//! `next_action` looks at the workflow and the event log and says what
//! the run does next: execute a node, emit a re-route, pause, or finish.
//! It performs no IO and holds no state of its own — the log is the
//! state (I2), which is what makes `yunta run` and `yunta resume` the
//! same code path: both just keep asking "what's next" until the answer
//! is terminal.
//!
//! M-0 cut: sequential execution (one node at a time; `max_parallel_nodes`
//! arrives with full T4.1), `on_interrupt: restart_node` as the only
//! policy (D99's alternatives arrive with T4.5 proper).

use yunta_core::events::{Event, EventPayload};
use yunta_core::{NodeId, Workflow};

use crate::replay::{derive, NodeState};

/// What the run does next. `Pause` covers every "wait for a human" case
/// (§11.2 re-routes exhausted, a failed node with no re-route — the
/// escalation gate itself is M5/T7.2); `Broken` reproduces replay's
/// diagnostic.
#[derive(Debug, Clone, PartialEq)]
pub enum NextAction {
    Execute {
        node: NodeId,
        attempt: u32,
    },
    Reroute {
        from: NodeId,
        to: NodeId,
        attempt: u32,
        max_reroutes: u32,
        cause: String,
    },
    Pause {
        reason: String,
    },
    Finish,
    Broken {
        diagnostic: String,
    },
}

/// Per-node bookkeeping that plain final state can't answer: how many
/// times it started, when it last failed/finished, and its re-routes.
#[derive(Debug, Default, Clone)]
struct NodeHistory {
    starts: u32,
    last_failed_seq: Option<u64>,
    last_finished_seq: Option<u64>,
    reroutes: u32,
    /// seq and destination of the last `node_rerouted` this node emitted.
    last_reroute: Option<(u64, NodeId)>,
}

pub fn next_action(workflow: &Workflow, events: &[Event]) -> NextAction {
    let state = derive(events);
    if let Some(diagnostic) = state.broken {
        return NextAction::Broken { diagnostic };
    }

    let mut history: std::collections::HashMap<NodeId, NodeHistory> = Default::default();
    for event in events {
        let Some(node_id) = &event.node_id else {
            continue;
        };
        let entry = history.entry(node_id.clone()).or_default();
        match &event.payload {
            EventPayload::NodeStarted(_) => entry.starts += 1,
            EventPayload::NodeFailed(_) => entry.last_failed_seq = Some(event.seq),
            EventPayload::NodeFinished(_) => entry.last_finished_seq = Some(event.seq),
            EventPayload::NodeRerouted(p) => {
                entry.reroutes += 1;
                entry.last_reroute = Some((event.seq, p.to_node.clone()));
            }
            _ => {}
        }
    }
    let history = history; // read-only from here
    let hist = |id: &NodeId| history.get(id).cloned().unwrap_or_default();

    // 1. An orphaned `running` node (crash/Ctrl-C with no terminal event)
    //    restarts first — §8.1, restart_node.
    for node in &workflow.nodes {
        if matches!(state.nodes.get(&node.id), Some(NodeState::Running { .. })) {
            return NextAction::Execute {
                node: node.id.clone(),
                attempt: hist(&node.id).starts + 1,
            };
        }
    }

    // 2. Resolve failures (§11.2): re-route, hand control to a pending
    //    corrective node, return control to a corrected node, or pause.
    for node in &workflow.nodes {
        let Some(NodeState::Failed { outcome, .. }) = state.nodes.get(&node.id) else {
            continue;
        };
        let h = hist(&node.id);
        let failed_seq = h.last_failed_seq.unwrap_or(0);

        let rerouted_for_this_failure = h
            .last_reroute
            .as_ref()
            .filter(|(seq, _)| *seq > failed_seq)
            .cloned();

        match rerouted_for_this_failure {
            None => {
                if let Some(on_failure) = &node.on_failure {
                    if h.reroutes < on_failure.max_reroutes {
                        return NextAction::Reroute {
                            from: node.id.clone(),
                            to: on_failure.goto.clone(),
                            attempt: h.reroutes + 1,
                            max_reroutes: on_failure.max_reroutes,
                            cause: outcome.clone(),
                        };
                    }
                    return NextAction::Pause {
                        reason: format!(
                            "node `{}` failed and its {} re-route(s) to `{}` are exhausted: {outcome}",
                            node.id, on_failure.max_reroutes, on_failure.goto
                        ),
                    };
                }
                return NextAction::Pause {
                    reason: format!("node `{}` failed: {outcome}", node.id),
                };
            }
            Some((reroute_seq, to)) => {
                let corrective = hist(&to);
                let corrective_finished_since = corrective
                    .last_finished_seq
                    .is_some_and(|seq| seq > reroute_seq);
                let corrective_failed_since = corrective
                    .last_failed_seq
                    .is_some_and(|seq| seq > reroute_seq);

                if corrective_finished_since {
                    // §11.2: destination completed — the failed node
                    // returns to ready and re-runs.
                    return NextAction::Execute {
                        node: node.id.clone(),
                        attempt: h.starts + 1,
                    };
                }
                if corrective_failed_since {
                    // The corrective node failed on its own; it is a
                    // Failed node itself and this same loop resolves it
                    // (its own on_failure, or pause) on its iteration.
                    continue;
                }
                // Re-route emitted, corrective node not run yet.
                return NextAction::Execute {
                    node: to.clone(),
                    attempt: corrective.starts + 1,
                };
            }
        }
    }

    // 3. First fresh node whose dependencies are all finished.
    for node in &workflow.nodes {
        if state.nodes.contains_key(&node.id) {
            continue; // finished, or failed-and-handled-above
        }
        let deps_finished = node
            .depends_on
            .iter()
            .all(|dep| matches!(state.nodes.get(dep), Some(NodeState::Finished { .. })));
        if deps_finished {
            return NextAction::Execute {
                node: node.id.clone(),
                attempt: 1,
            };
        }
    }

    // 4. Nothing runnable: either everything finished, or something is
    //    stuck behind a failure this pass already chose to leave failed.
    let all_finished = workflow
        .nodes
        .iter()
        .all(|node| matches!(state.nodes.get(&node.id), Some(NodeState::Finished { .. })));
    if all_finished {
        NextAction::Finish
    } else {
        NextAction::Pause {
            reason: "no node is runnable: pending nodes are blocked behind unresolved failures"
                .to_string(),
        }
    }
}
