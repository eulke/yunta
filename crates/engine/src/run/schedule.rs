//! The scheduler's decision function — pure.
//!
//! [`decide`] looks at the workflow, the state its log derives, and the
//! policy the run froze, and says what the run does next: execute a
//! batch of independently-ready nodes (up to `max_parallel_nodes`), emit
//! a re-route, pause, fail, or finish. It performs no IO and holds no
//! state of its own — the log is the state, which is what makes `yunta
//! run` and `yunta resume` the same code path: both just keep asking
//! "what's next" until the answer is terminal.
//!
//! Six questions, asked in order, each its own function: is a gate
//! mid-flight ([`gate_step`]), is a node waiting on a person
//! ([`waiting_step`]), did an answer leave a node owing its terminal
//! ([`answered_step`]), did a crash leave nodes running ([`orphan_step`]),
//! is there a failure to resolve ([`failure_step`]), and what is ready
//! now ([`ready_batch`]). The first that answers decides.
//!
//! Node states are covered in full: `waiting` is derived (a
//! published gate, or unanswered questions — sections 0/0b below) and
//! `skipped` is the render-side reading of a mode-excluded node (the
//! filter here + `status`'s own display), and what an included node
//! waits on under that mode comes from `crate::modes`. `on_interrupt` covers every
//! policy — `resume_session` orphans re-Execute exactly
//! like `restart_node` ones from this function's point of view; the
//! *dispatch* path (`node_exec`) is what continues the recorded session
//! instead of opening a new one. Concurrency is DAG-shaped fan-out only
//! (independent nodes with no `depends_on` relation to each other); it
//! does not cover `kind: parallel`'s named groups or a loop's own task
//! `concurrency:`, both separate mechanisms.

use std::collections::HashSet;

use yunta_core::events::{PauseReason, RerouteCause, ResumePolicy};
use yunta_core::{DefaultOnFailure, ModeName, Node, NodeId, NodeKind, OnInterrupt, Workflow};

use crate::modes::dependencies_in_mode;
use crate::replay::{NodeState, RunState};

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
pub enum Decision {
    /// One or more independently-ready nodes to execute concurrently —
    /// `(node, attempt)` pairs, in workflow declaration order.
    Execute(Vec<(NodeId, u32)>),
    Reroute {
        from: NodeId,
        to: NodeId,
        attempt: u32,
        max_reroutes: u32,
        cause: RerouteCause,
    },
    Pause {
        reason: PauseReason,
    },
    /// A node failed with no re-route of its own and the run's
    /// `defaults.on_failure` closes the run as failed rather than pausing:
    /// `abort` at the first such failure, `continue` once every node not
    /// blocked behind a failure has run. `reason` names a causing node,
    /// the same shape a `Pause` reason takes. The imperative shell writes
    /// `run_finished { terminal_state: Failed }` and stops.
    Fail {
        reason: String,
    },
    /// A node's re-routes are exhausted — the one pause that
    /// carries real options, since "retry the same
    /// destination once more" and "abort" are both well-defined here
    /// (unlike a plain failure with no `on_failure` at all, which stays
    /// `Pause`). The imperative shell builds the actual
    /// `GateWaitingPayload` from these facts and asks `HumanInteraction`
    /// — this function stays a pure read of the log, no I/O.
    GateExhaustedReroutes {
        node: NodeId,
        goto: NodeId,
        max_reroutes: u32,
        cause: RerouteCause,
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
    /// A non-gate node the log derives as waiting on the questions it
    /// asked: a `questions_asked` with no `questions_answered` after
    /// it. The imperative shell re-reads the questions from the
    /// artifact and puts them to `HumanInteraction` (`questions_exec`)
    /// — the ONE ask site for first run and resume alike. One at a
    /// time, same reasoning as the gate steps.
    AskQuestions {
        node: NodeId,
    },
    /// A node whose questions were answered and whose close still owes
    /// it a terminal. The answer reopened it exactly where asking left
    /// it, so what is left is the `node_finished` the close deferred —
    /// no session, no new attempt. The ask round, a resume after a
    /// crash between the answer and the terminal, and an answer a
    /// control-plane client pre-seeded all land here.
    FinishAnswered {
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

/// The nodes only a re-route or a gate's `on:` may ever start.
///
/// A node named only as an `on_failure.goto` or gate `on:` target — a
/// node outside the main path, existing only for this — declares no
/// `depends_on` of its own on purpose. Left unfiltered, that empty list
/// reads as trivially satisfied and the generic "fresh nodes" batch
/// schedules it exactly like a genuine independent root, regardless of
/// whether anything ever actually failed or chose that gate option.
///
/// A goto/`on:` target that is genuinely part of the main path stays
/// out of this set — declaring no `depends_on` is not itself the signal
/// (the workflow's own entry point never has one either): what marks a
/// node as reroute-*only* is that it is also an otherwise-isolated dead
/// end, so nothing in the ordinary DAG reaches it. `plan` in the
/// reference `on: { ajustar: plan }` pattern fails that test: `approve`
/// declares `depends_on: [plan]`, so `plan` is a real predecessor that
/// also happens to be a valid re-route target later.
fn reroute_only_targets(
    nodes: &[&Node],
    dependencies: &std::collections::HashMap<NodeId, Vec<NodeId>>,
) -> HashSet<NodeId> {
    let deps_of = |id: &NodeId| dependencies.get(id).map(Vec::as_slice).unwrap_or(&[]);
    nodes
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
        .filter(|id| deps_of(id).is_empty() && !nodes.iter().any(|n| deps_of(&n.id).contains(id)))
        .cloned()
        .collect()
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
            !is_gate(node) && matches!(state.nodes.state(&node.id), Some(NodeState::Running { .. }))
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

/// What the run was frozen with: the knobs every decision reads.
/// Built once, when the run wakes, and handed to [`decide`] unchanged
/// for the rest of the invocation — a decision that re-read them could
/// answer two different things about the same log.
#[derive(Debug, Clone)]
pub struct Policy {
    /// How many independently-ready nodes may run at once.
    pub max_parallel_nodes: u32,
    /// What a node with no `on_interrupt` of its own inherits.
    pub on_interrupt: OnInterrupt,
    /// What a failed node with no `on_failure` of its own does to the run.
    pub on_failure: DefaultOnFailure,
    /// The nodes this run's mode includes; `None` when it declares no
    /// modes and every node is in.
    pub mode_nodes: Option<HashSet<NodeId>>,
}

impl Policy {
    /// The knobs `manifest` froze, for the mode this invocation runs.
    /// Built once when the run wakes: a mode's node set is a pure
    /// function of the workflow and the mode name, and re-deriving it
    /// per decision would be the same answer at scheduler cost.
    pub fn of(manifest: &yunta_core::Manifest, mode_name: &ModeName) -> Self {
        Policy {
            max_parallel_nodes: manifest.max_parallel_nodes,
            on_interrupt: manifest.config.resolved_on_interrupt(),
            on_failure: manifest.config.resolved_on_failure(),
            mode_nodes: crate::modes::mode_included_nodes(&manifest.workflow, mode_name),
        }
    }
}

/// Everything the six questions read, resolved once so none of them
/// re-derives it: the nodes this mode includes, the dependencies that
/// implies, and which of them exist only as a re-route's destination.
struct Board<'a> {
    nodes: Vec<&'a Node>,
    dependencies: std::collections::HashMap<NodeId, Vec<NodeId>>,
    reroute_only: HashSet<NodeId>,
    state: &'a RunState,
    policy: &'a Policy,
}

impl<'a> Board<'a> {
    fn of(workflow: &'a Workflow, state: &'a RunState, policy: &'a Policy) -> Self {
        // A node this run's mode excludes is never scheduled and never
        // counted toward completion — `check`'s own `check_modes` already
        // guarantees no *included* node's `on_failure.goto` reaches outside
        // the mode. An excluded node's dependents (a mode's own examples:
        // `implement` depends_on the excluded `approve-plan` in "quick")
        // wait on what the excluded node itself waited on — a mode cuts
        // deliberation, never the order of the work around it.
        let mode_nodes = policy.mode_nodes.as_ref();
        let nodes: Vec<&Node> = workflow
            .nodes
            .iter()
            .filter(|n| mode_nodes.is_none_or(|set| set.contains(&n.id)))
            .collect();
        let dependencies = dependencies_in_mode(workflow, mode_nodes);
        let reroute_only = reroute_only_targets(&nodes, &dependencies);
        Board {
            nodes,
            dependencies,
            reroute_only,
            state,
            policy,
        }
    }

    fn deps_of(&self, id: &NodeId) -> &[NodeId] {
        self.dependencies.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    fn finished(&self, id: &NodeId) -> bool {
        matches!(self.state.nodes.state(id), Some(NodeState::Finished { .. }))
    }

    fn deps_satisfied(&self, node: &Node) -> bool {
        self.deps_of(&node.id).iter().all(|dep| self.finished(dep))
    }

    /// Only a re-route or a gate's `on:` may start this node.
    fn reroute_only(&self, id: &NodeId) -> bool {
        self.reroute_only.contains(id)
    }

    /// The attempt number a node's next start carries.
    fn next_attempt(&self, id: &NodeId) -> u32 {
        self.state.nodes.get(id).map_or(0, |r| r.attempts) + 1
    }

    /// How a ready or re-opened gate is driven: poll the handle it was
    /// published under, publish it for the first time, or ask here.
    fn drive_gate(&self, node: &Node) -> Decision {
        match (
            self.state
                .gates
                .last_external_ref(&node.id)
                .map(str::to_string),
            is_external_gate(node),
        ) {
            (Some(external_ref), _) => Decision::PollGate {
                node: node.id.clone(),
                external_ref,
            },
            (None, true) => Decision::PublishGate {
                node: node.id.clone(),
            },
            (None, false) => Decision::ResolveInternalGate {
                node: node.id.clone(),
            },
        }
    }
}

/// What the run does next. Pure: the same state and policy always
/// answer the same thing, which is what lets `resume` re-ask the
/// question a crashed invocation was in the middle of.
pub fn decide(workflow: &Workflow, state: &RunState, policy: &Policy) -> Decision {
    if let Some(diagnostic) = &state.broken {
        return Decision::Broken {
            diagnostic: diagnostic.clone(),
        };
    }
    let board = Board::of(workflow, state, policy);
    gate_step(&board)
        .or_else(|| waiting_step(&board))
        .or_else(|| answered_step(&board))
        .or_else(|| orphan_step(&board))
        .or_else(|| failure_step(&board))
        .unwrap_or_else(|| ready_batch(&board))
}

/// A `Running` gate node is never a crash orphan (`on_interrupt` is
/// about session-crash uncertainty, which a gate has none of — it isn't
/// a session). It reaches `Running` two ways, both resolved by
/// re-driving the gate, never by restarting it as an ordinary node: the
/// SHA-drift recheck re-opens a stale external approval (a recorded
/// `external_ref` → poll again), or a crash landed between the gate's
/// `node_started` and its `gate_waiting`/`gate_resolved` (no
/// `external_ref` yet → an external gate republishes, an internal one
/// asks again). Without this a crashed internal gate would fall through
/// every question below and the run would pause forever, never re-asked.
fn gate_step(board: &Board<'_>) -> Option<Decision> {
    let node = board.nodes.iter().copied().find(|node| {
        is_gate(node)
            && matches!(
                board.state.nodes.state(&node.id),
                Some(NodeState::Running { .. })
            )
    })?;
    Some(board.drive_gate(node))
}

/// Nodes the log derives as `waiting` — a human's move next, one at a
/// time: a published gate polls its forge, a node with declared
/// questions asks them, and anything else `Waiting` (only reachable
/// through a crash inside the tiny window between a paired
/// waiting/resolved emission) restarts like any orphan would.
fn waiting_step(board: &Board<'_>) -> Option<Decision> {
    for node in board.nodes.iter().copied() {
        let Some(NodeState::Waiting { external_ref }) = board.state.nodes.state(&node.id) else {
            continue;
        };
        if is_gate(node) {
            // A gate waiting with no recorded handle is republished
            // (external) or asked again (internal) rather than stuck.
            return Some(match (external_ref, is_external_gate(node)) {
                (Some(external_ref), _) => Decision::PollGate {
                    node: node.id.clone(),
                    external_ref: external_ref.clone(),
                },
                (None, true) => Decision::PublishGate {
                    node: node.id.clone(),
                },
                (None, false) => Decision::ResolveInternalGate {
                    node: node.id.clone(),
                },
            });
        }
        if node.asks() {
            return Some(Decision::AskQuestions {
                node: node.id.clone(),
            });
        }
        return Some(Decision::Execute(vec![(
            node.id.clone(),
            board.next_attempt(&node.id),
        )]));
    }
    None
}

/// A node whose questions were answered is `Running` again and owes the
/// terminal its close deferred. It is not an orphan: its work is done
/// and on the log, and restarting it would spend a session to redo what
/// the answer completed. Asked before [`orphan_step`] for exactly that
/// reason.
fn answered_step(board: &Board<'_>) -> Option<Decision> {
    let node = board
        .nodes
        .iter()
        .copied()
        .find(|node| board.state.answered_unfinished(&node.id))?;
    Some(Decision::FinishAnswered {
        node: node.id.clone(),
    })
}

/// Every orphaned `running` node (crash/Ctrl-C with no terminal event)
/// is resolved per its own `on_interrupt`: a node with no override
/// inherits the policy's. Any orphan resolving to `fail_if_uncertain`
/// pauses the whole resume rather than restarting even the
/// `restart_node` orphans alongside it — "never assume, never guess"
/// applies to the batch as a whole, not node by node. Orphans that DO
/// restart go together, already committed to running concurrently
/// before the crash, so capacity doesn't retroactively apply to how many
/// come back. Gate nodes never reach here ([`gate_step`] took them).
fn orphan_step(board: &Board<'_>) -> Option<Decision> {
    let orphaned = resume_policies(
        board.nodes.iter().copied(),
        board.state,
        board.policy.on_interrupt,
    );
    if orphaned.is_empty() {
        return None;
    }
    let uncertain: Vec<NodeId> = orphaned
        .iter()
        .filter(|policy| policy.on_interrupt == OnInterrupt::FailIfUncertain)
        .map(|policy| policy.node.clone())
        .collect();
    if !uncertain.is_empty() {
        return Some(Decision::Pause {
            reason: PauseReason::UncertainOrphans(uncertain),
        });
    }
    Some(Decision::Execute(
        orphaned
            .iter()
            .map(|policy| (policy.node.clone(), board.next_attempt(&policy.node)))
            .collect(),
    ))
}

/// Failures, one at a time: re-route, hand control to a pending
/// corrective node, return control to a corrected node, or stop. A
/// failure this call leaves unresolved is picked up again on the next
/// (the reroute or restart it emits changes the log, so the next call
/// sees a different answer for it).
fn failure_step(board: &Board<'_>) -> Option<Decision> {
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
                if let Some(decision) = unrerouted(board, node, failure, record.reroutes) {
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
/// the graph keeps running.
fn unrerouted(
    board: &Board<'_>,
    node: &Node,
    failure: &yunta_core::events::Failure,
    reroutes: u32,
) -> Option<Decision> {
    if let Some(on_failure) = &node.on_failure {
        if reroutes < on_failure.max_reroutes {
            return Some(Decision::Reroute {
                from: node.id.clone(),
                to: on_failure.goto.clone(),
                attempt: reroutes + 1,
                max_reroutes: on_failure.max_reroutes,
                cause: RerouteCause(failure.clone()),
            });
        }
        return Some(Decision::GateExhaustedReroutes {
            node: node.id.clone(),
            goto: on_failure.goto.clone(),
            max_reroutes: on_failure.max_reroutes,
            cause: RerouteCause(failure.clone()),
        });
    }
    let reason = PauseReason::NodeFailed {
        node: node.id.clone(),
        failure: failure.clone(),
    };
    match board.policy.on_failure {
        DefaultOnFailure::Pause => Some(Decision::Pause { reason }),
        DefaultOnFailure::Abort => Some(Decision::Fail {
            reason: reason.to_string(),
        }),
        DefaultOnFailure::Continue => None,
    }
}

/// A failure already re-routed to `to`: what happens next depends on
/// where that correction got to.
fn after_reroute(
    board: &Board<'_>,
    node: &Node,
    to: &NodeId,
    reroute_seq: yunta_core::Seq,
) -> Option<Decision> {
    let corrective = board.state.nodes.get(to).cloned().unwrap_or_default();
    if corrective
        .last_finished
        .is_some_and(|seq| seq > reroute_seq)
    {
        // The destination completed — the failed node returns to ready
        // and re-runs. A gate never goes through `Execute`: it re-asks
        // (internal) or re-polls/republishes (external).
        return Some(if is_gate(node) {
            board.drive_gate(node)
        } else {
            Decision::Execute(vec![(node.id.clone(), board.next_attempt(&node.id))])
        });
    }
    if corrective.last_failed.is_some_and(|seq| seq > reroute_seq) {
        // The corrective node failed on its own; it is a Failed node
        // itself and this same scan resolves it on its own turn.
        return None;
    }
    // Re-route emitted, corrective node not run yet.
    Some(Decision::Execute(vec![(
        to.clone(),
        board.next_attempt(to),
    )]))
}

/// What is ready now, and what it means when nothing is: every node
/// finished closes the run, and anything else is stuck behind something
/// this run left unresolved.
fn ready_batch(board: &Board<'_>) -> Decision {
    ready_gate(board)
        .or_else(|| ready_nodes(board))
        .unwrap_or_else(|| nothing_runnable(board))
}

/// A ready `kind: gate` is never batched with ordinary nodes — its
/// resolution is a forge round-trip, one at a time, same as every other
/// gate decision. A stateless ready gate is always unpublished
/// (external) or never-asked (internal); a published one carries
/// `Waiting` and [`waiting_step`] took it.
fn ready_gate(board: &Board<'_>) -> Option<Decision> {
    let node = board.nodes.iter().copied().find(|node| {
        is_gate(node)
            && !board.state.nodes.has_state(&node.id)
            && !board.reroute_only(&node.id)
            && board.deps_satisfied(node)
    })?;
    Some(board.drive_gate(node))
}

/// Fresh nodes whose dependencies are all finished, up to capacity, in
/// workflow declaration order.
fn ready_nodes(board: &Board<'_>) -> Option<Decision> {
    // A degenerate 0 would starve every ready node forever. `yunta
    // check` refuses it up front (`MaxParallelNodesZero`); this clamp
    // stays as defense in depth for a manifest frozen before that rule
    // existed — a stuck-looking run is worse than a sequential one.
    let capacity = board.policy.max_parallel_nodes.max(1) as usize;
    let batch: Vec<(NodeId, u32)> = board
        .nodes
        .iter()
        .copied()
        // finished, failed-and-handled above, a gate (handled above), or
        // a node only a re-route may start.
        .filter(|node| {
            !board.state.nodes.has_state(&node.id)
                && !is_gate(node)
                && !board.reroute_only(&node.id)
                && board.deps_satisfied(node)
        })
        .take(capacity)
        .map(|node| (node.id.clone(), 1))
        .collect();
    (!batch.is_empty()).then_some(Decision::Execute(batch))
}

/// Nothing is runnable: either every node this mode includes finished,
/// or something is stuck behind a failure this run left unresolved.
///
/// Only nodes the mode actually includes count toward completion — an
/// excluded node never reaches any terminal state (it is never scheduled
/// at all), so requiring it here would mean the run could never finish.
/// Same for an untouched reroute-only target that was simply never
/// needed: its source never failed, or the gate never chose its option.
fn nothing_runnable(board: &Board<'_>) -> Decision {
    let Some(stuck) = board.nodes.iter().copied().find(|node| {
        !board.finished(&node.id)
            && !(board.reroute_only(&node.id) && !board.state.nodes.has_state(&node.id))
    }) else {
        return Decision::Finish;
    };

    // Under `continue` a stuck run is the run's end — every node not
    // behind the failure has already run — so it closes failed, naming a
    // node that failed; under `pause` it freezes resumable.
    if board.policy.on_failure == DefaultOnFailure::Continue {
        if let Some(reason) =
            board
                .nodes
                .iter()
                .find_map(|node| match board.state.nodes.state(&node.id) {
                    Some(NodeState::Failed { failure, .. }) => Some(
                        PauseReason::NodeFailed {
                            node: node.id.clone(),
                            failure: failure.clone(),
                        }
                        .to_string(),
                    ),
                    _ => None,
                })
        {
            return Decision::Fail { reason };
        }
    }
    Decision::Pause {
        reason: PauseReason::Blocked {
            node: stuck.id.clone(),
            on: board
                .deps_of(&stuck.id)
                .iter()
                .filter(|dep| !board.finished(dep))
                .cloned()
                .collect(),
        },
    }
}
