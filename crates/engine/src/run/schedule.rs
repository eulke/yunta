//! The scheduler's decision function (T4.1 recorte) — pure.
//!
//! `next_step` looks at the workflow and the event log and says what the
//! run does next: execute a batch of independently-ready nodes (up to
//! `max_parallel_nodes`), emit a re-route, pause, or finish. It performs
//! no IO and holds no state of its own — the log is the state (I2), which
//! is what makes `yunta run` and `yunta resume` the same code path: both
//! just keep asking "what's next" until the answer is terminal.
//!
//! M-0/M4 cut: node states are `pending→ready→running→done|failed` only —
//! `skipped`/`waiting` wait for modes (M9) and gates (M5/T7.2), neither of
//! which exists yet. `on_interrupt: restart_node | fail_if_uncertain`
//! (T4.5, D99) — `resume_session` is real per the Contrato but has no
//! consumer (nothing resumes a session on crash recovery yet), so it
//! isn't in the schema at all rather than being accepted and ignored.
//! Concurrency is DAG-shaped fan-out only (independent nodes with no
//! `depends_on` relation to each other); it does not cover `kind:
//! parallel`'s named groups (T4.6) or a loop's own task `concurrency:`
//! (T5.10), both separate mechanisms per §5.5/§5.8.

use yunta_core::events::{Event, EventPayload};
use yunta_core::{Node, NodeId, NodeKind, OnInterrupt, Workflow};

use crate::replay::{derive, NodeState};

/// What the run does next. A batch of `Execute` entries is never empty
/// and is always homogeneous — the scheduler never mixes control actions
/// (`Reroute`/`Pause`/`Finish`/`Broken`) into the same step as node
/// executions, so the imperative shell only ever inspects one variant per
/// loop iteration.
#[derive(Debug, Clone, PartialEq)]
pub enum ScheduleStep {
    /// One or more independently-ready nodes to execute concurrently —
    /// `(node, attempt)` pairs, in workflow declaration order.
    Execute(Vec<(NodeId, u32)>),
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
    /// A node's re-routes are exhausted (§11.2) — the one pause this
    /// recorte gives real options for, since "retry the same
    /// destination once more" and "abort" are both well-defined here
    /// (unlike a plain failure with no `on_failure` at all, which stays
    /// `Pause`). The imperative shell builds the actual
    /// `GateWaitingPayload` from these facts and asks `HumanInteraction`
    /// (T7.2) — this function stays a pure read of the log, no I/O.
    GateExhaustedReroutes {
        node: NodeId,
        goto: NodeId,
        max_reroutes: u32,
        cause: String,
    },
    /// A `kind: gate` node is ready and has never been published (§5.6,
    /// T7.7) — the imperative shell commits its declared artifacts,
    /// opens the PR (or degrades to console without a forge), and
    /// pauses. One at a time, same reasoning as `GateExhaustedReroutes`:
    /// publishing is I/O this pure function only decides is needed.
    PublishGate {
        node: NodeId,
    },
    /// A `kind: gate` node has already been published and isn't
    /// resolved yet (or its prior approval needs re-checking against
    /// the PR's current head, T7.7's SHA-drift case) — the imperative
    /// shell polls the forge and maps aprobado/cambios/cerrado/pendiente
    /// (§5.6). `external_ref` is the forge's own handle, carried
    /// forward from this node's last `gate_waiting` event so the
    /// imperative shell never needs to re-derive it itself.
    PollGate {
        node: NodeId,
        external_ref: String,
    },
    Finish,
    Broken {
        diagnostic: String,
    },
}

/// Whether `node`'s `kind` is `gate` — the one kind whose "ready"/
/// "orphaned" handling never goes through the generic `Execute` path
/// (§5.6, T7.7): its resolution is a forge round-trip, not a session.
fn is_gate(node: &Node) -> bool {
    matches!(node.kind, NodeKind::Gate { .. })
}

/// The `external_ref` (forge handle) from this node's last `gate_waiting`
/// — `None` only if it was never published, which callers only reach
/// this for after confirming otherwise.
fn last_external_ref(events: &[Event], node_id: &NodeId) -> Option<String> {
    events.iter().rev().find_map(|e| match &e.payload {
        EventPayload::GateWaiting(p) if e.node_id.as_ref() == Some(node_id) => {
            p.external_ref.clone()
        }
        _ => None,
    })
}

/// Whether `node_id` has ever been published (§5.6) — a `gate_waiting`
/// on the log for it, regardless of resolution.
fn was_published(events: &[Event], node_id: &NodeId) -> bool {
    events.iter().any(|e| {
        matches!(&e.payload, EventPayload::GateWaiting(_)) && e.node_id.as_ref() == Some(node_id)
    })
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

pub fn next_step(
    workflow: &Workflow,
    events: &[Event],
    max_parallel_nodes: u32,
    default_on_interrupt: OnInterrupt,
) -> ScheduleStep {
    let state = derive(events);
    if let Some(diagnostic) = state.broken {
        return ScheduleStep::Broken { diagnostic };
    }

    // A degenerate 0 would starve every ready node forever, turning a
    // config mistake into a silent-looking stuck run instead of visible
    // progress — `yunta check` validating `max_parallel_nodes >= 1` is
    // still open debt (T1.3 doesn't cover `defaults:` semantics yet), so
    // the scheduler clamps rather than deadlock on it.
    let capacity = max_parallel_nodes.max(1) as usize;

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

    // 0. A `Running` gate node is never a crash orphan (§8.1's
    //    `on_interrupt` is about session-crash uncertainty, which a gate
    //    has none of — it isn't a session). It only reaches `Running`
    //    via T7.7's own SHA-drift recheck (re-opening a stale approval,
    //    the imperative shell's own doing, before this function ever
    //    runs) — resolve it the same way as any other unresolved,
    //    already-published gate: poll again.
    if let Some(node) = workflow.nodes.iter().find(|node| {
        is_gate(node) && matches!(state.nodes.get(&node.id), Some(NodeState::Running { .. }))
    }) {
        if let Some(external_ref) = last_external_ref(events, &node.id) {
            return ScheduleStep::PollGate {
                node: node.id.clone(),
                external_ref,
            };
        }
    }

    // 1. Every orphaned `running` node (crash/Ctrl-C with no terminal
    //    event) is resolved per its own `on_interrupt` (§8.1, D99): a node
    //    with no override inherits `default_on_interrupt`. Any orphan
    //    resolving to `fail_if_uncertain` pauses the whole resume rather
    //    than restarting even the `restart_node` orphans alongside it —
    //    "never assume, never guess" (§8.1) applies to the batch as a
    //    whole, not node by node. Orphans that DO restart go together,
    //    already committed to running concurrently before the crash, so
    //    capacity doesn't retroactively apply to how many come back.
    //    Gate nodes never reach here (handled in section 0 above).
    let orphaned: Vec<&Node> = workflow
        .nodes
        .iter()
        .filter(|node| {
            !is_gate(node) && matches!(state.nodes.get(&node.id), Some(NodeState::Running { .. }))
        })
        .collect();
    if !orphaned.is_empty() {
        let resolved = |node: &Node| node.on_interrupt.unwrap_or(default_on_interrupt);
        let uncertain: Vec<&NodeId> = orphaned
            .iter()
            .filter(|node| resolved(node) == OnInterrupt::FailIfUncertain)
            .map(|node| &node.id)
            .collect();
        if !uncertain.is_empty() {
            let names = uncertain
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return ScheduleStep::Pause {
                reason: format!(
                    "node(s) `{names}` were running with no terminal event when the engine \
                     last stopped — `on_interrupt: fail_if_uncertain` refuses to guess whether \
                     they finished; verify manually before resuming"
                ),
            };
        }
        let orphans: Vec<(NodeId, u32)> = orphaned
            .iter()
            .map(|node| (node.id.clone(), hist(&node.id).starts + 1))
            .collect();
        return ScheduleStep::Execute(orphans);
    }

    // 2. Resolve failures one at a time (§11.2): re-route, hand control to
    //    a pending corrective node, return control to a corrected node, or
    //    pause. A failure this iteration leaves unresolved is picked up
    //    again on the next (the reroute/restart it emits changes the log,
    //    so the next call sees a different answer for it).
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
                        return ScheduleStep::Reroute {
                            from: node.id.clone(),
                            to: on_failure.goto.clone(),
                            attempt: h.reroutes + 1,
                            max_reroutes: on_failure.max_reroutes,
                            cause: outcome.clone(),
                        };
                    }
                    return ScheduleStep::GateExhaustedReroutes {
                        node: node.id.clone(),
                        goto: on_failure.goto.clone(),
                        max_reroutes: on_failure.max_reroutes,
                        cause: outcome.clone(),
                    };
                }
                return ScheduleStep::Pause {
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
                    return ScheduleStep::Execute(vec![(node.id.clone(), h.starts + 1)]);
                }
                if corrective_failed_since {
                    // The corrective node failed on its own; it is a
                    // Failed node itself and this same loop resolves it
                    // (its own on_failure, or pause) on its iteration.
                    continue;
                }
                // Re-route emitted, corrective node not run yet.
                return ScheduleStep::Execute(vec![(to.clone(), corrective.starts + 1)]);
            }
        }
    }

    // 3. Fresh nodes whose dependencies are all finished, up to capacity.
    //    A ready `kind: gate` is never batched with ordinary nodes — its
    //    resolution is a forge round-trip, one at a time, same as
    //    section 0/1's own gate handling (§5.6, T7.7).
    for node in &workflow.nodes {
        if !is_gate(node) || state.nodes.contains_key(&node.id) {
            continue;
        }
        let deps_finished = node
            .depends_on
            .iter()
            .all(|dep| matches!(state.nodes.get(dep), Some(NodeState::Finished { .. })));
        if !deps_finished {
            continue;
        }
        return if was_published(events, &node.id) {
            match last_external_ref(events, &node.id) {
                Some(external_ref) => ScheduleStep::PollGate {
                    node: node.id.clone(),
                    external_ref,
                },
                // Published but the reference wasn't recorded (shouldn't
                // happen for a log this function itself would have
                // written) — republish rather than get stuck.
                None => ScheduleStep::PublishGate {
                    node: node.id.clone(),
                },
            }
        } else {
            ScheduleStep::PublishGate {
                node: node.id.clone(),
            }
        };
    }

    let mut batch = Vec::new();
    for node in &workflow.nodes {
        if batch.len() >= capacity {
            break;
        }
        if state.nodes.contains_key(&node.id) || is_gate(node) {
            continue; // finished, failed-and-handled-above, or a gate (handled above)
        }
        let deps_finished = node
            .depends_on
            .iter()
            .all(|dep| matches!(state.nodes.get(dep), Some(NodeState::Finished { .. })));
        if deps_finished {
            batch.push((node.id.clone(), 1));
        }
    }
    if !batch.is_empty() {
        return ScheduleStep::Execute(batch);
    }

    // 4. Nothing runnable: either everything finished, or something is
    //    stuck behind a failure this pass already chose to leave failed.
    let all_finished = workflow
        .nodes
        .iter()
        .all(|node| matches!(state.nodes.get(&node.id), Some(NodeState::Finished { .. })));
    if all_finished {
        ScheduleStep::Finish
    } else {
        ScheduleStep::Pause {
            reason: "no node is runnable: pending nodes are blocked behind unresolved failures"
                .to_string(),
        }
    }
}
