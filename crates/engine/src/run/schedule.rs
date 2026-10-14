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
//! Seven questions, asked in order, each its own function: is a gate
//! mid-flight ([`gate_step`]), is a node waiting on a person
//! ([`waiting_step`]), did an answer leave a node owing its terminal
//! ([`answered_step`]), did a crash leave nodes running ([`orphan_step`]),
//! is there a failure to resolve ([`failure_step`]), does an invariant's
//! pass no longer speak for the run's tree ([`reverify::reverify_step`]),
//! and what is ready now ([`ready_batch`]). The first that answers
//! decides.
//!
//! Node states are covered in full: `waiting` is derived (a
//! published gate, or unanswered questions) and
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

use yunta_core::events::{NodeWait, PauseReason, ResumePolicy};
use yunta_core::{Node, NodeId, NodeKind, OnInterrupt, Workflow};

mod baseline;
mod decision;
mod failure;
mod follow;
mod grant;
mod policy;
mod ready;
mod retry;
mod reverify;

pub use baseline::owed_baseline;
pub use decision::Decision;
use failure::failure_step;
pub use follow::follows_build;
pub(crate) use policy::person_may_grant_scope;
pub use policy::{next_mode_after, Policy};
use ready::ready_batch;

use crate::modes::dependencies_in_mode;
use crate::replay::{NodeState, RunState};

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
/// end, so nothing in the ordinary DAG reaches it. `plan` in the usual
/// `on: { adjust: plan }` pattern fails that test: `approve`
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

/// Everything the seven questions read, resolved once so none of them
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
        // the mode. An excluded node's dependents (for example `implement`,
        // which depends_on the excluded `approve-plan` in a mode that leaves
        // it out) wait on what the excluded node itself waited on — a mode
        // cuts deliberation, never the order of the work around it.
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

    /// Whether `id` waits for `on`, directly or through others.
    fn depends_on(&self, id: &NodeId, on: &NodeId) -> bool {
        let mut seen: HashSet<&NodeId> = HashSet::new();
        let mut stack: Vec<&NodeId> = self.deps_of(id).iter().collect();
        while let Some(dep) = stack.pop() {
            if dep == on {
                return true;
            }
            if seen.insert(dep) {
                stack.extend(self.deps_of(dep));
            }
        }
        false
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
    let next = gate_step(&board)
        .or_else(|| waiting_step(&board))
        .or_else(|| answered_step(&board))
        .or_else(|| orphan_step(&board))
        .or_else(|| failure_step(&board))
        .or_else(|| reverify::reverify_step(&board))
        .unwrap_or_else(|| ready_batch(&board));
    baseline::before(workflow, state, policy, next)
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
/// time: a published gate polls its forge, a node that asked asks
/// again, and anything else `Waiting` (only reachable through a crash
/// inside the tiny window between a paired waiting/resolved emission)
/// restarts like any orphan would.
///
/// What the node waits on decides, never what kind of node it is: a
/// node waiting on its own questions is never taken for a gate, however
/// it is declared.
fn waiting_step(board: &Board<'_>) -> Option<Decision> {
    for node in board.nodes.iter().copied() {
        let Some(NodeState::Waiting { on }) = board.state.nodes.state(&node.id) else {
            continue;
        };
        match on {
            // A gate waiting with no recorded handle is republished
            // (external) or asked again (internal) rather than stuck.
            // Whether it is external is a property of the declaration.
            NodeWait::Gate { external_ref } => {
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
            NodeWait::Questions { .. } => {
                return Some(Decision::AskQuestions {
                    node: node.id.clone(),
                });
            }
        }
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
            reason: PauseReason::UncertainOrphans { nodes: uncertain },
        });
    }
    Some(Decision::Execute(
        orphaned
            .iter()
            .map(|policy| (policy.node.clone(), board.next_attempt(&policy.node)))
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::next_mode_after;
    use yunta_core::persisted::{Persisted, PersistedDoc};

    #[test]
    fn a_persisted_mode_ladder_promotes_beta_to_gamma() {
        let workflow = yunta_core::yaml::parse(
            "name: ship\nmodes:\n  alpha: { include: all }\n  beta: { include: all }\n  gamma: { include: all }\nnodes:\n  - { id: review, kind: bash, run: 'true' }\n",
        ).expect("a mode ladder");
        let manifest = yunta_core::Manifest {
            schema_version: <yunta_core::Manifest as Persisted>::SCHEMA_VERSION,
            yunta_version: "0.0.5".to_string(),
            workflow,
            config: yunta_core::ConfigLayer::default(),
            inputs: std::collections::BTreeMap::new(),
            prompts: std::collections::BTreeMap::new(),
            base_branch: "main".to_string(),
            base_commit: "deadbeef".into(),
            isolation: yunta_core::Isolation::None,
            max_parallel_nodes: 1,
            workflow_hash: yunta_core::sha256_hex(b"workflow"),
            config_hash: yunta_core::sha256_hex(b"config"),
            paths: None,
            pack: None,
            project: None,
            adapter: None,
            frozen: None,
            provenance_gaps: Vec::new(),
        };
        let bytes = PersistedDoc::of(manifest).write().expect("manifest writes");
        let read = PersistedDoc::<yunta_core::Manifest>::read(&bytes).expect("manifest reads");
        assert_eq!(
            next_mode_after(&read.doc.workflow, &"beta".into()),
            Some("gamma".into()),
        );
    }
}
