//! One declared node as a frame: what the frozen workflow says it is,
//! what the log says it has done, and what it is doing at the instant
//! the caller reads it.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use chrono::{DateTime, Utc};
use surco::words::Standing;

use yunta_core::events::{ArtifactId, PauseReason, RunPhaseRaw, TaskStatus, TokenUsage};
use yunta_core::{Node, NodeId, NodeKind, OnFailure, TaskId};

use crate::live::{last_event_age, open_sessions, recent_tool_calls, OpenSession, ToolCall};
use crate::process_registry::Engine;
use crate::replay::{NodeState, RunState};
use crate::stats::NodeStat;
use yunta_core::events::{NodeRecord, Reroute, RerouteOrigin, ResolvedRunner};

use super::Included;

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
    /// Where the node stands in the one vocabulary every surface says it
    /// in — its row, the counter beside it, its JSON — decided here, once.
    pub standing: Standing,
    /// What stands behind that word: the mode or the birth that left the
    /// node out, or what replay derived from the log.
    pub state: NodeStanding,
    /// The runner this node resolved through, as `runner_resolved`
    /// recorded it: the name, the candidate chosen (its adapter, model
    /// and agent), and every candidate passed over with the reason. A
    /// runner that fell back to a later candidate is therefore visible,
    /// which the rule that every runner fallback is frozen in the manifest
    /// and recorded with its cause demands of it. `None` until the node
    /// resolves one.
    pub runner: Option<ResolvedRunner>,
    /// The highest attempt number the log reached for this node; `None`
    /// for a node that never started.
    pub attempt: Option<u32>,
    /// How long this node has spent working: every stretch of work that
    /// closed, plus the one still open at the instant the run's clock reads:
    /// now while the run is going, its last applied event once it has
    /// finished or broken. Time spent waiting on a person is not work and is
    /// left out. `None` for a node that never started.
    pub elapsed: Option<Duration>,
    /// How long ago this node wrote its last event: measured, it grows while the
    /// node says nothing. `None` for a node with no events.
    pub last_event_age: Option<Duration>,
    /// How many events of this node the log holds.
    pub events: u64,
    /// What this node's attempts have spent, first and retries alike. A child
    /// run's spend is in no node's row, only in
    /// [`RunFrame::tokens`](super::RunFrame::tokens).
    pub tokens: TokenUsage,
    /// Every artifact this node produced, by the identity the log holds
    /// it under, in the order the log accepted them.
    ///
    /// An identity rather than a path: the bytes live in the run's store
    /// by hash and the directory is a view of it, so what a node
    /// produced is a fact of the log and not of the filesystem.
    pub artifacts: Vec<ArtifactId>,
    /// The tasks this node has in `running`, by id.
    pub running_tasks: Vec<RunningTask>,
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
    /// How many times its own `on_failure` re-routed it: what its
    /// `max_reroutes` counts. A person's choice at a gate that sends the run
    /// somewhere is a decision, not counted here.
    pub reroutes: u32,
    /// How long a person was asked about this node, booked as the run's
    /// clock books a person's time, the stretch open at the frame's instant
    /// included. Zero for a node no person was asked about.
    pub people: Duration,
    /// How many times a person decided this node's gate: its own
    /// resolutions, each round a choice sends back counted.
    pub decisions: usize,
    /// How many rounds of questions this node asked a person.
    pub question_rounds: usize,
    /// What the frozen workflow declares of it that the log cannot say
    /// before it runs.
    pub declares: Declares,
}

impl NodeFrame {
    /// Whether the time this node's row shows is a person's: a gate a
    /// person was asked about waits on that person and does no work of its
    /// own.
    pub fn person_took(&self) -> bool {
        self.declares.gate.is_some() && !self.people.is_zero()
    }

    /// The rounds a person has had with this node: for a gate, each time a
    /// person decided it and the round it waits in now; for any other node,
    /// its rounds of questions.
    pub fn person_rounds(&self) -> usize {
        match self.declares.gate {
            Some(_) => self.decisions + usize::from(self.standing == Standing::NeedsYou),
            None => self.question_rounds,
        }
    }
}

/// A task in `running`, and whether an agent is on it: one no
/// session has opened for since it started running is having its
/// criteria checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningTask {
    pub id: TaskId,
    pub in_session: bool,
}

/// What the frozen workflow declares of a node that a reader wants before
/// the node runs: where its failure goes, and what it will ask a person.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Declares {
    /// Where its `on_failure` sends a failure, and how many times at most.
    pub on_failure: Option<OnFailure>,
    /// What it asks a person, for a gate.
    pub gate: Option<GateAsks>,
}

/// What a gate declares it asks a person.
#[derive(Debug, Clone, PartialEq)]
pub struct GateAsks {
    /// Its question as written; `None` where it writes none, and asks
    /// whether to approve what it shows, or for a decision by its node.
    pub question: Option<String>,
    /// Whether a forge carries the gate: its `external:` names one, so a
    /// person approves the pull request it opens.
    pub on_forge: bool,
}

impl Declares {
    /// What `node` declares.
    fn of(node: &Node) -> Self {
        let gate = match &node.kind {
            NodeKind::Gate {
                message, external, ..
            } => Some(GateAsks {
                question: message.clone().filter(|message| !message.trim().is_empty()),
                on_forge: external.is_some(),
            }),
            _ => None,
        };
        Declares {
            on_failure: node.on_failure.clone(),
            gate,
        }
    }
}

/// Where one declared node stands — the three cases a frozen DAG and a
/// log produce together, as an enum so no pair of flags can say two
/// things at once.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeStanding {
    /// The run's mode leaves this node out: it never runs in this run.
    Skipped,
    /// The run left this optional node out, for the reason it records:
    /// the project lacks what it needs, or only such a node leads to it.
    LeftOut(yunta_core::Because),
    /// Included, and the log has not started it.
    ToGo,
    /// Its attempt `attempt` is open while the run is paused: nothing runs
    /// it, and the run's resume starts it again.
    Paused { attempt: u32 },
    /// What replay derives from the log, with the node's outcome, its
    /// failure, or the gate it waits on.
    Reached(NodeState),
}

impl NodeStanding {
    /// The word a node standing here is called by on its own, before what
    /// the rest of its run says about it: a correction under way, a reset
    /// the run waits on, a question asked about it at a terminal.
    pub fn word(&self) -> Standing {
        match self {
            NodeStanding::Skipped | NodeStanding::LeftOut(_) => Standing::Skipped,
            NodeStanding::ToGo => Standing::NeverRan,
            NodeStanding::Paused { .. } => Standing::Stopped,
            NodeStanding::Reached(state) => node_word(state),
        }
    }
}

/// The word a node in `state`, as replay derived it, is called by on its
/// own — what a moment of the log says of it as it happened: a node on a
/// gate or on unanswered questions needs a person.
pub fn node_word(state: &NodeState) -> Standing {
    match state {
        NodeState::Running { .. } => Standing::Running,
        NodeState::Finished { .. } => Standing::Finished,
        NodeState::Failed { .. } => Standing::Failed,
        NodeState::Waiting { .. } => Standing::NeedsYou,
    }
}

/// Whether `record`'s latest failure was handed on to be corrected: it
/// failed, and its own `on_failure` — or a person choosing to retry it —
/// sent control to its correction after that failure. Until the node runs
/// again its failure is being worked on, not standing.
fn sent_to_correction(record: &NodeRecord) -> bool {
    let failed = matches!(record.state, Some(NodeState::Failed { .. }));
    match (&record.last_reroute, record.last_failed) {
        (Some(reroute), Some(failed_at)) => {
            failed && reroute.origin == RerouteOrigin::OnFailure && reroute.seq > failed_at
        }
        _ => false,
    }
}

/// The derivations every node frame is read from, and the instant they
/// are read at.
///
/// The run-wide ones — the per-node stats and the ids the mode includes —
/// are computed once for the whole frame instead of once per node. What a
/// node is doing right now is read off the same state the frame was given:
/// the node ledger already folds it.
pub(super) struct Reading<'a> {
    pub(super) stats: HashMap<&'a NodeId, &'a NodeStat>,
    /// Each node's share of a person's time, from the run's stats.
    pub(super) people: &'a BTreeMap<NodeId, Duration>,
    pub(super) included: Included,
    /// What the run's birth left out, as `run_created` recorded it.
    pub(super) left_out: &'a [yunta_core::LeftOut],
    pub(super) state: &'a RunState,
    pub(super) now: DateTime<Utc>,
    /// Whether the run has not closed: a correction under way belongs to
    /// a run that can still reach it.
    pub(super) open: bool,
    /// The engine driving the run, as its registry said when the caller
    /// read it.
    pub(super) engine: &'a Engine,
}

impl Reading<'_> {
    /// Frames one node of the frozen DAG, `group` being the `parallel`
    /// group it is declared inside.
    pub(super) fn node(&self, node: &Node, group: Option<&Node>) -> NodeFrame {
        let stat = self.stats.get(&node.id);
        let record = self.state.nodes.get(&node.id);
        let gate = self.state.gates.get(&node.id);
        let state = self.standing(node, group);
        let correcting = self.open && record.is_some_and(sent_to_correction);
        NodeFrame {
            id: node.id.clone(),
            kind: node.kind.kind_name(),
            group: group.map(|group| group.id.clone()),
            standing: self.word(&node.id, &state, correcting),
            state,
            runner: record.and_then(|record| record.runner.clone()),
            attempt: stat.map(|stat| stat.attempts),
            elapsed: stat.map(|stat| stat.active_so_far()),
            last_event_age: last_event_age(self.state, &node.id, self.now),
            events: self.state.events_of(&node.id),
            tokens: stat.map(|stat| stat.tokens).unwrap_or_default(),
            artifacts: self
                .state
                .artifacts
                .by_producer(&node.id)
                .map(|artifact| artifact.artifact.clone())
                .collect(),
            running_tasks: self.running_tasks(&node.id),
            sessions: open_sessions(self.state, &node.id),
            // No cap here: the frame carries the attempt's calls and the
            // surface trims them to what it can draw.
            activity: recent_tool_calls(self.state, &node.id, usize::MAX),
            reroute: record.and_then(|record| record.last_reroute.clone()),
            reroutes: record.map_or(0, |record| record.on_failure_reroutes),
            people: self.people.get(&node.id).copied().unwrap_or_default(),
            decisions: gate.map_or(0, |gate| gate.own_resolutions().count()),
            question_rounds: gate.map_or(0, |gate| gate.rounds.len()),
            declares: Declares::of(node),
        }
    }

    /// The word `node` is called by, decided once from where the mode, the
    /// run's birth and the log put it (`state`), a correction under way, a
    /// reset the run waits on, and a question the process driving the run
    /// is asking about it.
    ///
    /// A failed node a re-route is correcting is `rerouted`: counted as
    /// failed it would alarm a reader about a correction the workflow
    /// planned for. A node the run waits on a reset for is `waiting`, which
    /// a clock ends. A node a person is being asked about at a terminal
    /// needs them, whatever its log says, and so does the gate a run is
    /// parked on.
    fn word(&self, node: &NodeId, state: &NodeStanding, correcting: bool) -> Standing {
        let reset = self.pause().and_then(PauseReason::reset_wait);
        match state {
            NodeStanding::Skipped | NodeStanding::LeftOut(_) => state.word(),
            // A gate asks before its log says anything of it.
            _ if self.engine.asks_about(node) => Standing::NeedsYou,
            NodeStanding::ToGo if self.pause().and_then(PauseReason::gate) == Some(node) => {
                Standing::NeedsYou
            }
            NodeStanding::Paused { .. } if reset.is_some() => Standing::Waiting,
            NodeStanding::Reached(NodeState::Failed { .. }) if correcting => Standing::Rerouted,
            NodeStanding::Reached(NodeState::Failed { .. })
                if reset.is_some_and(|(held, _)| held == node) =>
            {
                Standing::Waiting
            }
            _ => state.word(),
        }
    }

    /// The pause the run stands in, while it stands in one.
    fn pause(&self) -> Option<&PauseReason> {
        match self.state.run.phase() {
            RunPhaseRaw::Paused => self.state.run.paused().map(|(reason, _)| reason),
            _ => None,
        }
    }

    /// Where the node stands: the mode decides first — an excluded node
    /// never runs, whatever a log says — then what the run left out at
    /// birth, and replay decides the rest.
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
        if let Some(left) = self.left_out.iter().find(|left| left.node == *top_level) {
            return NodeStanding::LeftOut(left.because.clone());
        }
        match self.state.nodes.state(&node.id) {
            Some(NodeState::Running { attempt })
                if *self.state.run.phase() == RunPhaseRaw::Paused =>
            {
                NodeStanding::Paused { attempt: *attempt }
            }
            Some(derived) => NodeStanding::Reached(derived.clone()),
            None => NodeStanding::ToGo,
        }
    }

    /// The tasks `node` has in `running`, by id — sorted, so two
    /// frames of the same log list them in the same order.
    fn running_tasks(&self, node: &NodeId) -> Vec<RunningTask> {
        self.state
            .tasks
            .iter()
            .filter(|(_, record)| {
                matches!(record.status, TaskStatus::Running) && record.owner.as_ref() == Some(node)
            })
            .map(|(task, record)| RunningTask {
                id: task.clone(),
                in_session: record.in_session,
            })
            .collect()
    }
}
