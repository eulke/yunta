//! The scheduler's decision function — pure.
//!
//! `next_step` looks at the workflow and the event log and says what the
//! run does next: execute a batch of independently-ready nodes (up to
//! `max_parallel_nodes`), emit a re-route, pause, or finish. It performs
//! no IO and holds no state of its own — the log is the state, which
//! is what makes `yunta run` and `yunta resume` the same code path: both
//! just keep asking "what's next" until the answer is terminal.
//!
//! Node states are covered in full: `waiting` is derived (a
//! published gate, or unanswered questions — sections 0/0b below) and
//! `skipped` is the render-side reading of a mode-excluded node (the
//! filter here + `status`'s own display), and what an included node
//! waits on under that mode comes from `crate::modes`. `on_interrupt` covers the
//! Contrato's full triple — `resume_session` orphans re-Execute exactly
//! like `restart_node` ones from this function's point of view; the
//! *dispatch* path (`node_exec`) is what continues the recorded session
//! instead of opening a new one. Concurrency is DAG-shaped fan-out only
//! (independent nodes with no `depends_on` relation to each other); it
//! does not cover `kind: parallel`'s named groups or a loop's own task
//! `concurrency:`, both separate mechanisms.

use std::collections::HashSet;

use yunta_core::events::{EventPayload, ResumePolicy, StoredEvent};
use yunta_core::{ModeName, Node, NodeId, NodeKind, OnInterrupt, Seq, Workflow};

use crate::modes::dependencies_in_mode;
use crate::replay::{derive, NodeState, RunState};

/// The mode immediately after `mode_name` in `modes:`'s own declaration
/// order — the *only* direction promotion ever moves (going back to an
/// earlier mode in the declaration doesn't exist), and the smallest
/// possible escalation past the current one, rather than jumping
/// straight to whichever mode a human might name off-hand.
/// `None` when the workflow declares no modes, the current mode name
/// isn't one of them (the `"default"` sentinel, or a stale/renamed
/// mode), or it's already the last one declared — nothing to promote
/// to, so promotion is never offered.
pub fn next_mode_after(workflow: &Workflow, mode_name: &ModeName) -> Option<ModeName> {
    let modes = workflow.modes.as_ref()?;
    let index = modes.get_index_of(mode_name)?;
    modes.get_index(index + 1).map(|(name, _)| name.clone())
}

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
    /// A node's re-routes are exhausted — the one pause this
    /// recorte gives real options for, since "retry the same
    /// destination once more" and "abort" are both well-defined here
    /// (unlike a plain failure with no `on_failure` at all, which stays
    /// `Pause`). The imperative shell builds the actual
    /// `GateWaitingPayload` from these facts and asks `HumanInteraction`
    /// — this function stays a pure read of the log, no I/O.
    GateExhaustedReroutes {
        node: NodeId,
        goto: NodeId,
        max_reroutes: u32,
        cause: String,
    },
    /// A `kind: gate` node is ready and has never been published —
    /// the imperative shell commits its declared artifacts,
    /// opens the PR (or degrades to console without a forge), and
    /// pauses. One at a time, same reasoning as `GateExhaustedReroutes`:
    /// publishing is I/O this pure function only decides is needed.
    PublishGate {
        node: NodeId,
    },
    /// A `kind: gate` node has already been published and isn't
    /// resolved yet (or its prior approval needs re-checking against
    /// the PR's current head — the SHA-drift case) — the imperative
    /// shell polls the forge and maps approved/changes-requested/
    /// closed/pending. `external_ref` is the forge's own handle, carried
    /// forward from this node's last `gate_waiting` event so the
    /// imperative shell never needs to re-derive it itself.
    PollGate {
        node: NodeId,
        external_ref: String,
    },
    /// An internal gate (`external: None`) is ready (or came
    /// back after its chosen option's re-route completed): the
    /// imperative shell builds the escalation object from
    /// `message`/`options`/`on` and asks `HumanInteraction`. One at a
    /// time, same reasoning as the other gate steps.
    ResolveInternalGate {
        node: NodeId,
    },
    /// A non-gate node the log derives as waiting-on-questions: its
    /// `kind: questions` artifact has no `questions_answered` yet. The
    /// imperative shell re-reads the questions from the artifact and
    /// puts them to `HumanInteraction` (`questions_exec`) — the ONE ask
    /// site for first run and resume alike. One at a time, same
    /// reasoning as the gate steps.
    AskQuestions {
        node: NodeId,
    },
    Finish,
    Broken {
        diagnostic: String,
    },
}

/// Whether `node`'s `kind` is `gate` — the one kind whose "ready"/
/// "orphaned" handling never goes through the generic `Execute` path:
/// its resolution is a forge round-trip, not a session.
fn is_gate(node: &Node) -> bool {
    matches!(node.kind, NodeKind::Gate { .. })
}

/// The orphans a resume finds among `nodes` — left `running` in the
/// log with no terminal event, gates excepted (a running gate is a
/// question still open, not a crash) — each with the `on_interrupt`
/// it resolves to: its own declaration, or `default_on_interrupt`.
/// The one place that resolution is made; `run_resumed` records it
/// and the scheduler acts on it.
pub(crate) fn resume_policies<'a>(
    nodes: impl IntoIterator<Item = &'a Node>,
    state: &RunState,
    default_on_interrupt: OnInterrupt,
) -> Vec<ResumePolicy> {
    nodes
        .into_iter()
        .filter(|node| {
            !is_gate(node) && matches!(state.nodes.get(&node.id), Some(NodeState::Running { .. }))
        })
        .map(|node| ResumePolicy {
            node: node.id.clone(),
            on_interrupt: node.on_interrupt.unwrap_or(default_on_interrupt),
        })
        .collect()
}

/// Whether a gate node has an `external:` block (forge-published)
/// — decides which of the gate steps serves it.
fn is_external_gate(node: &Node) -> bool {
    matches!(
        &node.kind,
        NodeKind::Gate {
            external: Some(_),
            ..
        }
    )
}

/// Whether `node` declares a `kind: questions` artifact — what routes a
/// `Waiting` non-gate node to `AskQuestions` instead of an
/// orphan-style restart.
fn declares_questions(node: &Node) -> bool {
    node.artifacts.as_ref().is_some_and(|artifacts| {
        artifacts.produces.iter().any(|spec| {
            matches!(
                spec,
                yunta_core::ArtifactSpec::Typed {
                    kind: yunta_core::ArtifactKind::Questions,
                    ..
                }
            )
        })
    })
}

/// The `external_ref` (forge handle) from this node's last `gate_waiting`
/// — `None` only if it was never published, which callers only reach
/// this for after confirming otherwise.
fn last_external_ref(events: &[StoredEvent], node_id: &NodeId) -> Option<String> {
    events.iter().rev().find_map(|e| match e.payload() {
        Some(EventPayload::GateWaiting(p)) if e.node_id.as_ref() == Some(node_id) => {
            p.external_ref.clone()
        }
        _ => None,
    })
}

/// Per-node bookkeeping that plain final state can't answer: how many
/// times it started, when it last failed/finished, and its re-routes.
#[derive(Debug, Default, Clone)]
struct NodeHistory {
    starts: u32,
    last_failed_seq: Option<Seq>,
    last_finished_seq: Option<Seq>,
    reroutes: u32,
    /// seq and destination of the last `node_rerouted` this node emitted.
    last_reroute: Option<(Seq, NodeId)>,
}

pub fn next_step(
    workflow: &Workflow,
    events: &[StoredEvent],
    max_parallel_nodes: u32,
    default_on_interrupt: OnInterrupt,
    mode_nodes: Option<&HashSet<NodeId>>,
) -> ScheduleStep {
    let state = derive(events);
    if let Some(diagnostic) = state.broken {
        return ScheduleStep::Broken { diagnostic };
    }

    // A node this run's mode excludes is never scheduled and never
    // counted toward completion — `check`'s own `check_modes` already
    // guarantees no *included* node's `on_failure.goto` reaches outside
    // the mode. An excluded node's dependents (a mode's own examples:
    // `implement` depends_on the excluded `approve-plan` in "quick")
    // wait on what the excluded node itself waited on — a mode cuts
    // deliberation, never the order of the work around it.
    let nodes: Vec<&Node> = workflow
        .nodes
        .iter()
        .filter(|n| mode_nodes.is_none_or(|set| set.contains(&n.id)))
        .collect();
    let dependencies = dependencies_in_mode(workflow, mode_nodes);
    let deps_of = |id: &NodeId| dependencies.get(id).map(Vec::as_slice).unwrap_or(&[]);
    let deps_satisfied = |node: &Node| {
        deps_of(&node.id)
            .iter()
            .all(|dep| matches!(state.nodes.get(dep), Some(NodeState::Finished { .. })))
    };

    // A node named only as an `on_failure.goto` or gate `on:`
    // target — a node outside the main path, existing only for this —
    // declares no `depends_on` of its own on purpose. Left unfiltered,
    // that empty list reads as trivially satisfied and the generic
    // "fresh nodes" batch below schedules it exactly like a genuine
    // independent root, regardless of whether anything ever actually
    // failed or chose that gate option. Reachability for these nodes
    // comes exclusively from the `Reroute`/gate-`on:` schedule steps
    // (sections 2 and the gate-resolution paths above), so they're
    // pulled out of the generic scan entirely.
    //
    // A goto/`on:` target that's genuinely part of the main path stays
    // untouched — declaring no `depends_on` isn't itself the signal
    // (the workflow's own entry point never has one either): what marks
    // a node as reroute-*only* is that it's also an otherwise-isolated
    // dead end — no other node's `depends_on` names it, so nothing in
    // the ordinary DAG ever reaches it either. `plan` in the reference
    // `on: { ajustar: plan }` pattern fails this: `approve` itself
    // declares `depends_on: [plan]`, so `plan` is a real predecessor on
    // the main path that also happens to be a valid re-route target
    // later, not a node existing solely for the re-route.
    let goto_or_gate_on_targets: HashSet<&NodeId> = nodes
        .iter()
        .copied()
        .flat_map(|n| {
            let goto = n.on_failure.iter().map(|of| &of.goto);
            let gate_on = match &n.kind {
                NodeKind::Gate { on, .. } => on.values().collect::<Vec<_>>(),
                _ => Vec::new(),
            };
            goto.chain(gate_on)
        })
        .collect();
    let has_forward_dependent = |id: &NodeId| nodes.iter().any(|n| deps_of(&n.id).contains(id));
    let is_reroute_only_target = |id: &NodeId| {
        goto_or_gate_on_targets.contains(id) && !has_forward_dependent(id) && deps_of(id).is_empty()
    };

    // A degenerate 0 would starve every ready node forever. `yunta
    // check` refuses it up front (`MaxParallelNodesZero`);
    // this clamp stays as defense in depth for a manifest frozen before
    // that rule existed — a stuck-looking run is worse than a
    // sequential one either way.
    let capacity = max_parallel_nodes.max(1) as usize;

    let mut history: std::collections::HashMap<NodeId, NodeHistory> = Default::default();
    for event in events {
        let Some(node_id) = &event.node_id else {
            continue;
        };
        let entry = history.entry(node_id.clone()).or_default();
        match event.payload() {
            Some(EventPayload::NodeStarted(_)) => entry.starts += 1,
            Some(EventPayload::NodeFailed(_)) => entry.last_failed_seq = Some(event.seq),
            Some(EventPayload::NodeFinished(_)) => entry.last_finished_seq = Some(event.seq),
            Some(EventPayload::NodeRerouted(p)) => {
                entry.reroutes += 1;
                entry.last_reroute = Some((event.seq, p.to_node.clone()));
            }
            _ => {}
        }
    }
    let history = history; // read-only from here
    let hist = |id: &NodeId| history.get(id).cloned().unwrap_or_default();

    // 0. A `Running` gate node is never a crash orphan (`on_interrupt`
    //    is about session-crash uncertainty, which a gate has none of —
    //    it isn't a session). It only reaches `Running` via the
    //    SHA-drift recheck (re-opening a stale approval, the imperative
    //    shell's own doing, before this function ever runs) — resolve
    //    it the same way as any other unresolved, already-published
    //    gate: poll again.
    if let Some(node) = nodes.iter().copied().find(|node| {
        is_gate(node) && matches!(state.nodes.get(&node.id), Some(NodeState::Running { .. }))
    }) {
        if let Some(external_ref) = last_external_ref(events, &node.id) {
            return ScheduleStep::PollGate {
                node: node.id.clone(),
                external_ref,
            };
        }
    }

    // 0b. Nodes the log derives as `waiting` — a human's
    //     move next, one at a time: a published gate polls its forge, a
    //     node with declared questions asks them, and anything else
    //     Waiting (only reachable through a crash inside the tiny
    //     window between a paired waiting/resolved emission) restarts
    //     like any orphan would.
    for node in nodes.iter().copied() {
        let Some(NodeState::Waiting { external_ref }) = state.nodes.get(&node.id) else {
            continue;
        };
        if is_gate(node) {
            return match (external_ref, is_external_gate(node)) {
                (Some(external_ref), _) => ScheduleStep::PollGate {
                    node: node.id.clone(),
                    external_ref: external_ref.clone(),
                },
                // External, waiting with no recorded forge handle —
                // republish rather than get stuck.
                (None, true) => ScheduleStep::PublishGate {
                    node: node.id.clone(),
                },
                // Internal (only reachable through a crash between its
                // synchronous waiting/resolved pair) — ask again.
                (None, false) => ScheduleStep::ResolveInternalGate {
                    node: node.id.clone(),
                },
            };
        }
        if declares_questions(node) {
            return ScheduleStep::AskQuestions {
                node: node.id.clone(),
            };
        }
        return ScheduleStep::Execute(vec![(node.id.clone(), hist(&node.id).starts + 1)]);
    }

    // 1. Every orphaned `running` node (crash/Ctrl-C with no terminal
    //    event) is resolved per its own `on_interrupt`: a node
    //    with no override inherits `default_on_interrupt`. Any orphan
    //    resolving to `fail_if_uncertain` pauses the whole resume rather
    //    than restarting even the `restart_node` orphans alongside it —
    //    "never assume, never guess" applies to the batch as a
    //    whole, not node by node. Orphans that DO restart go together,
    //    already committed to running concurrently before the crash, so
    //    capacity doesn't retroactively apply to how many come back.
    //    Gate nodes never reach here (handled in section 0 above).
    let orphaned = resume_policies(nodes.iter().copied(), &state, default_on_interrupt);
    if !orphaned.is_empty() {
        let uncertain: Vec<&NodeId> = orphaned
            .iter()
            .filter(|policy| policy.on_interrupt == OnInterrupt::FailIfUncertain)
            .map(|policy| &policy.node)
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
            .map(|policy| (policy.node.clone(), hist(&policy.node).starts + 1))
            .collect();
        return ScheduleStep::Execute(orphans);
    }

    // 2. Resolve failures one at a time: re-route, hand control to
    //    a pending corrective node, return control to a corrected node, or
    //    pause. A failure this iteration leaves unresolved is picked up
    //    again on the next (the reroute/restart it emits changes the log,
    //    so the next call sees a different answer for it).
    for node in nodes.iter().copied() {
        let Some(NodeState::Failed { outcome, .. }) = state.nodes.get(&node.id) else {
            continue;
        };
        let h = hist(&node.id);
        let failed_seq = h.last_failed_seq;

        let rerouted_for_this_failure = h
            .last_reroute
            .as_ref()
            .filter(|(seq, _)| Some(*seq) > failed_seq)
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
                    // Destination completed — the failed node
                    // returns to ready and re-runs. A gate never goes
                    // through Execute: it re-asks (internal) or
                    // re-polls/republishes (external).
                    if is_gate(node) {
                        return if !is_external_gate(node) {
                            ScheduleStep::ResolveInternalGate {
                                node: node.id.clone(),
                            }
                        } else {
                            match last_external_ref(events, &node.id) {
                                Some(external_ref) => ScheduleStep::PollGate {
                                    node: node.id.clone(),
                                    external_ref,
                                },
                                None => ScheduleStep::PublishGate {
                                    node: node.id.clone(),
                                },
                            }
                        };
                    }
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
    //    section 0/1's own gate handling.
    for node in nodes.iter().copied() {
        if !is_gate(node) || state.nodes.contains_key(&node.id) {
            continue;
        }
        if is_reroute_only_target(&node.id) || !deps_satisfied(node) {
            continue;
        }
        // A published gate carries `Waiting` state and is
        // handled in section 0b — a stateless ready gate here is
        // always unpublished (external) or never-asked (internal).
        return if is_external_gate(node) {
            ScheduleStep::PublishGate {
                node: node.id.clone(),
            }
        } else {
            ScheduleStep::ResolveInternalGate {
                node: node.id.clone(),
            }
        };
    }

    let mut batch = Vec::new();
    for node in nodes.iter().copied() {
        if batch.len() >= capacity {
            break;
        }
        if state.nodes.contains_key(&node.id) || is_gate(node) {
            continue; // finished, failed-and-handled-above, or a gate (handled above)
        }
        if is_reroute_only_target(&node.id) {
            continue; // only `Reroute`/gate-`on:` may start this node
        }
        if deps_satisfied(node) {
            batch.push((node.id.clone(), 1));
        }
    }
    if !batch.is_empty() {
        return ScheduleStep::Execute(batch);
    }

    // 4. Nothing runnable: either everything finished, or something is
    //    stuck behind a failure this pass already chose to leave failed.
    //    Only nodes this mode actually includes count — an excluded node
    //    never reaches any terminal state (it's never scheduled at
    //    all), so requiring it here would mean the run could never finish.
    //    Same reasoning applies to an untouched reroute-only target that
    //    was simply never needed (its source never failed, or the gate
    //    never chose its option): it never reaches a terminal state
    //    either — counting it here would make an ordinary green run
    //    un-finishable.
    let all_finished = nodes.iter().all(|node| {
        matches!(state.nodes.get(&node.id), Some(NodeState::Finished { .. }))
            || (is_reroute_only_target(&node.id) && !state.nodes.contains_key(&node.id))
    });
    if all_finished {
        ScheduleStep::Finish
    } else {
        ScheduleStep::Pause {
            reason: "no node is runnable: pending nodes are blocked behind unresolved failures"
                .to_string(),
        }
    }
}
