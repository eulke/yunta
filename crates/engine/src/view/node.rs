//! One declared node as a frame: what the frozen workflow says it is,
//! what the log says it has done, and what it is doing at the instant
//! the caller reads it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};

use yunta_core::events::{NodeReroutedPayload, RerouteOrigin, StoredEvent, TaskStatus, TokenUsage};
use yunta_core::{Node, NodeId, TaskId};

use crate::live::{last_event_age, open_sessions, recent_tool_calls, OpenSession, ToolCall};
use crate::replay::{NodeState, RunState};
use crate::runner::ResolvedRunner;
use crate::stats::NodeStat;

use super::{Included, Walk};

/// One declared node as it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeFrame {
    pub id: NodeId,
    /// The `kind:` the frozen workflow declares, never one inferred from
    /// an event: what a node *is* comes from the manifest, what it *did*
    /// comes from the log.
    pub kind: &'static str,
    /// The enclosing `parallel` group, `None` for a top-level node —
    /// what lets a surface draw a group and its children as one unit.
    pub group: Option<NodeId>,
    pub state: NodeStanding,
    /// The runner this node resolved through, as `runner_resolved`
    /// recorded it: the name, the candidate chosen (its adapter, model
    /// and agent), and every candidate passed over with the reason. A
    /// runner that fell back to a later candidate is therefore visible,
    /// which is what invariant I17 demands of it. `None` until the node
    /// resolves one.
    pub runner: Option<ResolvedRunner>,
    /// The highest attempt number the log reached for this node; `None`
    /// for a node that never started.
    pub attempt: Option<u32>,
    /// How long this node has spent running: every attempt that closed,
    /// plus the one open at `now`. `None` for a node that never started.
    pub elapsed: Option<Duration>,
    /// How long ago this node wrote its last event — what a view shows
    /// instead of a spinner, since it is measured and grows while the
    /// node says nothing. `None` for a node with no events.
    pub last_event_age: Option<Duration>,
    /// What this node's closed attempts reported spending, first and
    /// retries alike. The attempt now running reports into the run's own
    /// live total instead, so these never add up to it.
    pub tokens: TokenUsage,
    /// Every `artifact_written` path, in log order.
    pub artifacts: Vec<PathBuf>,
    /// The ledger tasks this node has in `running`, by id.
    pub running_tasks: Vec<TaskId>,
    /// The sessions open on the attempt now running, oldest first — one
    /// per task in the batch for a `loop` node above concurrency 1.
    pub sessions: Vec<OpenSession>,
    /// The tool calls of the attempt now running, newest first; a
    /// surface shows as many of them as it has room for.
    ///
    /// These are the **node's** calls, never one session's. The event
    /// envelope carries `run_id`, `seq`, `timestamp` and `node_id` and
    /// nothing that names a session, so at loop concurrency above one no
    /// log can say which task session made a call: the limit is the
    /// event schema's, not this derivation's.
    pub activity: Vec<ToolCall>,
    /// The last re-route this node took; `None` for one that never
    /// rerouted.
    pub reroute: Option<Reroute>,
}

/// Where one declared node stands — the three cases a frozen DAG and a
/// log produce together, as an enum so no pair of flags can say two
/// things at once.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeStanding {
    /// The run's mode leaves this node out: it never runs in this run.
    Skipped,
    /// Included, and the log has not started it.
    ToGo,
    /// What replay derives from the log, with the node's outcome, its
    /// failure, or the gate it waits on.
    Reached(NodeState),
}

/// A re-route as `node_rerouted` recorded it, on the node it left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reroute {
    pub to: NodeId,
    pub cause: String,
    /// The retry count and the cap it counts against — both `None` for a
    /// gate's routing choice, which is a decision, not a retry.
    pub attempt: Option<u32>,
    pub max: Option<u32>,
    pub origin: RerouteOrigin,
    pub at: DateTime<Utc>,
}

impl Reroute {
    pub(super) fn of(payload: &NodeReroutedPayload, at: DateTime<Utc>) -> Self {
        Reroute {
            to: payload.to_node.clone(),
            cause: payload.cause.clone(),
            attempt: payload.attempt,
            max: payload.max_reroutes,
            origin: payload.origin,
            at,
        }
    }
}

/// The derivations every node frame is read from, and the instant they
/// are read at.
///
/// The run-wide ones — replay's state, the per-node stats, the log walk,
/// the ids the mode includes — are derived once for the whole frame
/// instead of once per node. The log itself is kept beside them because
/// the live readings ([`last_event_age`], [`open_sessions`],
/// [`recent_tool_calls`]) answer a question about one node and read it
/// per node.
pub(super) struct Reading<'a> {
    pub(super) stats: HashMap<&'a NodeId, &'a NodeStat>,
    pub(super) included: Included,
    pub(super) walk: &'a Walk,
    pub(super) state: &'a RunState,
    pub(super) events: &'a [StoredEvent],
    pub(super) now: DateTime<Utc>,
}

impl Reading<'_> {
    /// Frames one node of the frozen DAG, `group` being the `parallel`
    /// group it is declared inside.
    pub(super) fn node(&self, node: &Node, group: Option<&Node>) -> NodeFrame {
        let stat = self.stats.get(&node.id);
        NodeFrame {
            id: node.id.clone(),
            kind: node.kind.kind_name(),
            group: group.map(|group| group.id.clone()),
            state: self.standing(node, group),
            runner: self.walk.runner.get(&node.id).cloned(),
            attempt: stat.map(|stat| stat.attempts),
            elapsed: stat.map(|stat| stat.active_so_far()),
            last_event_age: last_event_age(self.events, &node.id, self.now),
            tokens: stat.map(|stat| stat.tokens).unwrap_or_default(),
            artifacts: self
                .state
                .artifacts
                .get(&node.id)
                .cloned()
                .unwrap_or_default(),
            running_tasks: self.running_tasks(&node.id),
            sessions: open_sessions(self.events, &node.id),
            // No cap here: the frame carries the attempt's calls and the
            // surface trims them to what it can draw.
            activity: recent_tool_calls(self.events, &node.id, usize::MAX),
            reroute: self.walk.reroute.get(&node.id).cloned(),
        }
    }

    /// Where the node stands: the mode decides first — an excluded node
    /// never runs, whatever a log says — and replay decides the rest.
    ///
    /// A `parallel` group's children stand with their group: `modes:`
    /// names top-level nodes, and a group the mode schedules runs every
    /// child it declares.
    fn standing(&self, node: &Node, group: Option<&Node>) -> NodeStanding {
        let top_level = group.map_or(&node.id, |group| &group.id);
        if self
            .included
            .as_ref()
            .is_some_and(|ids| !ids.contains(top_level))
        {
            return NodeStanding::Skipped;
        }
        match self.state.nodes.get(&node.id) {
            Some(derived) => NodeStanding::Reached(derived.clone()),
            None => NodeStanding::ToGo,
        }
    }

    /// The ledger tasks `node` has in `running`, by id — sorted, so two
    /// frames of the same log list them in the same order.
    fn running_tasks(&self, node: &NodeId) -> Vec<TaskId> {
        let mut tasks: Vec<TaskId> = self
            .state
            .tasks
            .iter()
            .filter(|(task, status)| {
                matches!(status, TaskStatus::Running)
                    && self.state.task_nodes.get(*task) == Some(node)
            })
            .map(|(task, _)| task.clone())
            .collect();
        tasks.sort();
        tasks
    }
}
