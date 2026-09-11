//! [`RunFrame`] — one derived snapshot of a run, ready for any surface
//! to present.
//!
//! The same family as [`derive`] and
//! [`compute_run_stats_at`](crate::compute_run_stats_at): a pure
//! function of the run's own event log, the workflow its manifest
//! froze, and one instant the caller supplies. Nothing here reads a
//! clock, touches storage, or knows what the answer gets drawn with — a
//! terminal region, a `--json` document and a control-plane tool read
//! the same frame, so no two of them can disagree about what a run's
//! progress means.
//!
//! **Composition is a tree, never an average** (`contrato-del-run.md`
//! §8.5): a child run's events live under their own `run_id`, in a log
//! this frame never opens. A parent carries the [`ChildLink`]s its own
//! log recorded, and the caller frames each child from the child's own
//! log and manifest.

mod node;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Utc};

use yunta_core::events::{
    run_mode, ChildRunFinishedPayload, EventPayload, Failure, StoredEvent, TaskStatus,
    TerminalState, TokenUsage,
};
use yunta_core::{AdapterId, Capability, ModeName, NodeId, RunId, Workflow};

use crate::history::PriorEstimation;
use crate::live::live_total_tokens;
use crate::modes::mode_included_nodes;
use crate::replay::{derive, unknown_kind_counts, NodeState, RunState, UnknownKindCount};
use crate::runner::ResolvedRunner;
use crate::stats::compute_run_stats_at;

use node::Reading;

pub use node::{NodeFrame, NodeStanding, Reroute};

/// Work split by where it stands, at one level: the DAG's nodes, or the
/// ledger's tasks.
///
/// **No method divides `done` by `total`.** A percentage lies the moment
/// a re-route grows the denominator, which is why D45 and
/// `contrato-del-run.md` §8.5 forbid one; leaving the division out of
/// the type beats trusting every surface to remember.
///
/// **Nothing here drops in silence.** A task that fails at integration
/// returns to `ready` (§5.5) and a re-routed node runs again, so a naive
/// done-over-total walks backwards. Split into buckets, that same event
/// reads as a move *between* them — `failed` down one, `running` up one
/// — which a surface can show and attribute to the event that caused
/// it. The buckets partition the total:
/// `done + failed + running + waiting + to_go == total`.
///
/// `skipped` sits outside that sum, because a node the run's mode leaves
/// out is not work this run will do; `skipped_by` names the mode that
/// left it out, so a narrowed denominator is never a silent one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Counter {
    pub done: usize,
    pub failed: usize,
    pub running: usize,
    /// Parked on something outside the work itself: a node on a person
    /// (a published gate, unanswered questions), a task on the
    /// dependencies it declares.
    pub waiting: usize,
    /// Included and not begun.
    pub to_go: usize,
    /// Everything this run will do at this level, `skipped` excluded.
    pub total: usize,
    /// Nodes the run's mode leaves out, counted so the narrowing shows.
    /// Always zero for tasks: a mode narrows the graph, never a ledger.
    pub skipped: usize,
    /// The mode that skipped them; `None` when nothing is skipped.
    pub skipped_by: Option<ModeName>,
}

/// Where the run as a whole stands, derived from the log alone.
#[derive(Debug, Clone, PartialEq)]
pub enum RunPhase {
    /// The log carries no start yet.
    Created,
    Running,
    /// Parked on a person: at least one node is on a published gate or
    /// on unanswered questions, or the run itself paused for a decision.
    /// A run whose other nodes keep moving reports this too — what is
    /// running is in [`RunFrame::flow`] beside what is parked, so the
    /// two are read together.
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
    /// them is in [`RunFrame::nodes`] and counted in [`RunFrame::flow`].
    Node {
        node: NodeId,
        external_ref: Option<String>,
    },
    /// The run itself paused, with the reason `run_paused` recorded — an
    /// exhausted budget, a cancellation, a gate nobody has answered.
    Run { reason: String },
}

/// A run as it stands at one instant: every field a pure function of
/// that run's log, the workflow frozen in its manifest, and the `now`
/// the caller supplies.
#[derive(Debug, Clone, PartialEq)]
pub struct RunFrame {
    pub run_id: RunId,
    /// The frozen workflow's own `name:`.
    pub workflow: String,
    /// The mode frozen in `run_created`.
    pub mode: ModeName,
    pub phase: RunPhase,
    /// The run's wall-clock: to `now` while it has not finished, to its
    /// last event once it has. `None` for an empty log.
    pub elapsed: Option<Duration>,
    /// The DAG's nodes.
    pub flow: Counter,
    /// The ledger's tasks — `None` until a `task_registered` arrives, so
    /// a run with no ledger reports no ledger instead of `0/0`.
    pub tasks: Option<Counter>,
    /// Every `node_rerouted` on the log: the count that makes a grown
    /// denominator attributable.
    pub reroutes: usize,
    /// What the run has spent, the work still in flight included and
    /// counted once ([`live_total_tokens`]). Deliberately *not* the sum
    /// of [`NodeFrame::tokens`], which covers closed attempts only.
    pub tokens: TokenUsage,
    /// What this workflow's past runs cost, from the caller that read
    /// the history ([`prior_estimation`](crate::prior_estimation)) —
    /// this run's own log cannot know it. `None` when the caller has
    /// none to hand over.
    pub prior: Option<PriorEstimation>,
    /// Every node the frozen DAG declares, in declaration order, each
    /// `parallel` group followed by its own children.
    pub nodes: Vec<NodeFrame>,
    pub children: Vec<ChildLink>,
    /// Every `capability_degraded`, in log order: what the engine asked
    /// an adapter for, and what happened instead.
    pub degraded: Vec<Degradation>,
    /// Events under kinds this binary does not know, by kind. A surface
    /// that hides them misreports a log it half understands.
    pub unknown_kinds: Vec<UnknownKindCount>,
}

/// A child run this run gave birth to, as a link — never as numbers
/// averaged into the parent's counters (`contrato-del-run.md` §8.5).
///
/// The child's events are under its own `run_id`, and its workflow
/// *name* is on no event at all: `child_run_created` records the link
/// and the child's workflow hash, never its name (`run/workflow_exec.rs`
/// states it). A caller that labels a child reads the child's own frozen
/// manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildLink {
    pub run_id: RunId,
    /// The parent's `kind: workflow` node that bore it; `None` for a
    /// link the log recorded under no node.
    pub node: Option<NodeId>,
    /// How the child closed, or `None` while it is still open.
    pub terminal: Option<TerminalState>,
}

/// One `capability_degraded`: the capability the engine consulted, the
/// adapter that does not declare it, and the policy applied instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Degradation {
    pub capability: Capability,
    pub adapter: AdapterId,
    pub policy: String,
    pub node: Option<NodeId>,
    pub at: DateTime<Utc>,
}

/// Derives one run's frame from its log, the workflow its manifest
/// froze, the history the caller read, and the instant it is looked at.
///
/// Pure: same inputs, same frame, always. `now` is the caller's own
/// clock, injected — nothing in this module reads one — so a frame
/// rendered at a chosen instant is reproducible in a test.
pub fn run_frame(
    run_id: &RunId,
    workflow: &Workflow,
    events: &[StoredEvent],
    prior: Option<&PriorEstimation>,
    now: DateTime<Utc>,
) -> RunFrame {
    let state = derive(events);
    let stats = compute_run_stats_at(workflow, events, now);
    let walk = walk_log(events);
    let mode = run_mode(events);
    let reading = Reading {
        stats: stats
            .nodes
            .iter()
            .map(|stat| (&stat.node_id, stat))
            .collect(),
        included: mode_included_nodes(workflow, &mode),
        walk: &walk,
        state: &state,
        events,
        now,
    };
    let nodes: Vec<NodeFrame> = workflow
        .iter_nodes_with_group()
        .map(|(node, group)| reading.node(node, group))
        .collect();

    RunFrame {
        run_id: run_id.clone(),
        workflow: workflow.name.clone(),
        mode: mode.clone(),
        phase: phase(workflow, &state, events),
        elapsed: stats.wall_clock,
        flow: flow_counter(&nodes, &mode),
        tasks: task_counter(&state),
        reroutes: walk.reroutes,
        tokens: live_total_tokens(events),
        prior: prior.cloned(),
        nodes,
        children: walk.children,
        degraded: walk.degraded,
        unknown_kinds: unknown_kind_counts(&state),
    }
}

/// The flow counter, folded from the very node frames a surface shows
/// beside it, so the count and the list under it cannot disagree.
fn flow_counter(nodes: &[NodeFrame], mode: &ModeName) -> Counter {
    let mut counter = Counter::default();
    for node in nodes {
        match &node.state {
            NodeStanding::Skipped => {
                counter.skipped += 1;
                continue;
            }
            NodeStanding::ToGo => counter.to_go += 1,
            NodeStanding::Reached(NodeState::Finished { .. }) => counter.done += 1,
            NodeStanding::Reached(NodeState::Failed { .. }) => counter.failed += 1,
            NodeStanding::Reached(NodeState::Running { .. }) => counter.running += 1,
            NodeStanding::Reached(NodeState::Waiting { .. }) => counter.waiting += 1,
        }
        counter.total += 1;
    }
    counter.skipped_by = (counter.skipped > 0).then(|| mode.clone());
    counter
}

/// The ledger's counter, or `None` until a task is registered: a run
/// with no ledger reports no ledger, never a fabricated `0/0`.
///
/// `blocked` is the tasks' own `waiting`: a task held by the
/// dependencies it declares is parked, not running.
fn task_counter(state: &RunState) -> Option<Counter> {
    if state.tasks.is_empty() {
        return None;
    }
    let mut counter = Counter {
        total: state.tasks.len(),
        ..Counter::default()
    };
    for status in state.tasks.values() {
        match status {
            TaskStatus::Done => counter.done += 1,
            TaskStatus::Failed => counter.failed += 1,
            TaskStatus::Running => counter.running += 1,
            TaskStatus::Blocked => counter.waiting += 1,
            TaskStatus::Pending | TaskStatus::Ready => counter.to_go += 1,
        }
    }
    Some(counter)
}

/// The run's phase: a log that stopped making sense says so first, then
/// the run's own close, then what it is parked on, then movement.
fn phase(workflow: &Workflow, state: &RunState, events: &[StoredEvent]) -> RunPhase {
    if let Some(diagnostic) = &state.broken {
        return RunPhase::Broken {
            diagnostic: diagnostic.clone(),
        };
    }
    let last = events.iter().rev().find_map(|event| match event.payload() {
        Some(
            payload @ (EventPayload::RunFinished(_)
            | EventPayload::RunPaused(_)
            | EventPayload::RunResumed(_)
            | EventPayload::NodeStarted(_)),
        ) => Some(payload),
        _ => None,
    });
    match last {
        Some(EventPayload::RunFinished(p)) => closed(&p.terminal_state, events),
        // A node parked on a person is the more precise answer than the
        // pause reason that names the same wait in prose.
        Some(EventPayload::RunPaused(p)) => RunPhase::Waiting {
            on: waiting_node(workflow, state).unwrap_or_else(|| WaitingOn::Run {
                reason: p.reason.clone(),
            }),
        },
        Some(EventPayload::RunResumed(_) | EventPayload::NodeStarted(_)) => {
            match waiting_node(workflow, state) {
                Some(on) => RunPhase::Waiting { on },
                None => RunPhase::Running,
            }
        }
        _ => RunPhase::Created,
    }
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
/// order.
fn waiting_node(workflow: &Workflow, state: &RunState) -> Option<WaitingOn> {
    workflow
        .iter_nodes()
        .find_map(|node| match state.nodes.get(&node.id) {
            Some(NodeState::Waiting { external_ref }) => Some(WaitingOn::Node {
                node: node.id.clone(),
                external_ref: external_ref.clone(),
            }),
            _ => None,
        })
}

/// What one pass over the log collects that replay and stats do not: the
/// runner each node resolved through, the last re-route each one took,
/// the run's re-route count, its child links and its degradations.
#[derive(Default)]
struct Walk {
    runner: HashMap<NodeId, ResolvedRunner>,
    reroute: HashMap<NodeId, Reroute>,
    reroutes: usize,
    children: Vec<ChildLink>,
    degraded: Vec<Degradation>,
}

fn walk_log(events: &[StoredEvent]) -> Walk {
    let mut walk = Walk::default();
    for event in events {
        match event.payload() {
            Some(EventPayload::RunnerResolved(p)) => {
                if let Some(node) = &event.node_id {
                    walk.runner.insert(
                        node.clone(),
                        ResolvedRunner {
                            runner: p.runner.clone(),
                            chosen: p.chosen.clone(),
                            discarded: p.discarded.clone(),
                        },
                    );
                }
            }
            Some(EventPayload::NodeRerouted(p)) => {
                walk.reroutes += 1;
                if let Some(node) = &event.node_id {
                    walk.reroute
                        .insert(node.clone(), Reroute::of(p, event.timestamp));
                }
            }
            Some(EventPayload::ChildRunCreated(p)) => walk.children.push(ChildLink {
                run_id: p.child_run_id.clone(),
                node: event.node_id.clone(),
                terminal: None,
            }),
            Some(EventPayload::ChildRunFinished(p)) => walk.close_child(p, event),
            Some(EventPayload::CapabilityDegraded(p)) => walk.degraded.push(Degradation {
                capability: p.capability,
                adapter: p.adapter.clone(),
                policy: p.policy_applied.clone(),
                node: event.node_id.clone(),
                at: event.timestamp,
            }),
            _ => {}
        }
    }
    walk
}

impl Walk {
    /// Closes the link the child's birth opened, or records an already
    /// closed one for a log that carries a child's close without it.
    fn close_child(&mut self, payload: &ChildRunFinishedPayload, event: &StoredEvent) {
        let terminal = Some(payload.terminal_state);
        match self
            .children
            .iter_mut()
            .find(|link| link.run_id == payload.child_run_id)
        {
            Some(link) => link.terminal = terminal,
            None => self.children.push(ChildLink {
                run_id: payload.child_run_id.clone(),
                node: event.node_id.clone(),
                terminal,
            }),
        }
    }
}

/// The top-level node ids a run's mode schedules, as
/// [`mode_included_nodes`] answers it: `None` when nothing narrows the
/// graph at all.
type Included = Option<HashSet<NodeId>>;
