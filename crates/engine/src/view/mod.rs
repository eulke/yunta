//! [`RunFrame`] — one derived snapshot of a run, ready for any surface
//! to present.
//!
//! The same family as [`derive()`] and
//! [`compute_run_stats_at`](crate::compute_run_stats_at): a pure
//! function of the run's own event log, the workflow its manifest
//! froze, and one instant the caller supplies. Nothing here reads a
//! clock, touches storage, or knows what the answer gets drawn with — a
//! terminal region, a `--json` document and a control-plane tool read
//! the same frame, so no two of them can disagree about what a run's
//! progress means.
//!
//! **Composition is a tree, never an average**: a child run's events live
//! under their own `run_id`, in a log this frame never opens. A parent
//! carries the [`ChildLink`]s its own log recorded, and the caller frames
//! each child from the child's own log and manifest.

mod chronicle;
mod node;
mod phase;

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Utc};
use surco::words::Standing;

use yunta_core::events::{FindingSeverity, StoredEvent, TaskStatus, TokenUsage};
use yunta_core::{ModeName, NodeId, RunId, Workflow, WorkflowName};

use crate::history::PriorEstimation;
use crate::modes::mode_included_nodes;
use crate::process_registry::{Engine, EngineLiveness};
use crate::replay::{derive, unknown_kind_counts, RunState, UnknownKindCount};
use crate::stats::stats_observed_at;

use node::Reading;

pub use chronicle::{chronicle, Happening, Moment};
pub use node::{node_word, Declares, GateAsks, NodeFrame, NodeStanding, RunningTask};

pub use phase::{RunPhase, WaitingOn};
pub use yunta_core::events::{ChildLink, Degradation, Reroute};

/// The DAG's nodes split by where each stands: the word its row says.
///
/// **No method divides finished by `total`.** A percentage lies the moment
/// a re-route grows the denominator, so leaving the division out of the
/// type beats trusting every surface to remember.
///
/// **Nothing here drops in silence.** A re-routed node runs again, so a
/// naive finished-over-total walks backwards. Split by standing, that same
/// event reads as a move *between* places — `rerouted` down one, `running`
/// up one — which a surface can show and attribute to the event that
/// caused it. The places partition the total: `by` sums to `total`, and a
/// node is counted under the word its own row is drawn with.
///
/// `skipped` sits outside that sum, because a node the run's mode leaves
/// out is not work this run will do; `skipped_by` names the mode that
/// left it out, so a narrowed denominator is never a silent one.
/// `left_out` sits outside it too: an optional node the project cannot
/// run is not work this run will do either.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Counter {
    /// How many nodes this run will do stand in each place, in the order
    /// of [`Standing::ALL`]; a place no node stands in is left out.
    pub by: Vec<(Standing, usize)>,
    /// Everything this run will do, `skipped` and `left_out` excluded.
    pub total: usize,
    /// Nodes the run's mode leaves out, counted so the narrowing shows.
    pub skipped: usize,
    /// The mode that skipped them; `None` when nothing is skipped.
    pub skipped_by: Option<ModeName>,
    /// Optional nodes the run leaves out because the project lacks what
    /// they need, with the nodes only they lead to.
    pub left_out: usize,
}

impl Counter {
    /// How many nodes stand where `standing` says.
    pub fn of(&self, standing: Standing) -> usize {
        self.by
            .iter()
            .find(|(place, _)| *place == standing)
            .map_or(0, |(_, count)| *count)
    }
}

/// The run's tasks split by their ledger's status, the words a task is
/// called by.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaskCounter {
    pub done: usize,
    pub failed: usize,
    pub running: usize,
    /// Stopped with a cause the task cannot pass on its own.
    pub blocked: usize,
    /// Registered and not begun.
    pub to_go: usize,
    pub total: usize,
}

/// A run as it stands at one instant: every field a pure function of
/// that run's log, the workflow frozen in its manifest, and the `now`
/// the caller supplies.
#[derive(Debug, Clone, PartialEq)]
pub struct RunFrame {
    pub run_id: RunId,
    /// The frozen workflow's own `name:`.
    pub workflow: WorkflowName,
    /// The mode frozen in `run_created`.
    pub mode: ModeName,
    /// Where the run stands in the one vocabulary every surface says it in
    /// — a listing's row, a page's headline, its JSON, the exit code of the
    /// invocation that drove it — decided here, once.
    pub standing: Standing,
    /// What stands behind that word: the close, the pause, the movement.
    pub phase: RunPhase,
    /// The run's wall-clock: to `now` while it has not finished, to its
    /// last event once it has. `None` for an empty log.
    pub elapsed: Option<Duration>,
    /// The DAG's nodes.
    pub flow: Counter,
    /// The run's tasks — `None` until a `task_registered` arrives, so
    /// a run with no tasks reports no tasks instead of `0/0`.
    pub tasks: Option<TaskCounter>,
    /// Every `node_rerouted` on the log: the count that makes a grown
    /// denominator attributable.
    pub reroutes: usize,
    /// What the run has spent: every settled amount and the usage of the
    /// attempts still open, counted once ([`RunState::spend`]). Not the sum
    /// of [`NodeFrame::tokens`]: child runs add to this total, and no node
    /// row carries their spend.
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
    /// How many blocking findings the run carries, deduplicated. On the
    /// frame rather than left to each surface, because counting them
    /// means replaying the log, and the frame is the one pass every
    /// surface reads from.
    pub blocking_findings: usize,
}

/// A child run this run gave birth to, as a link — never as numbers
/// averaged into the parent's counters.
///
/// Derives one run's frame from its log, the workflow its manifest
/// froze, the history the caller read, what its registry says about the
/// engine driving it, and the instant it is looked at.
///
/// Pure: same inputs, same frame, always. `now` is the caller's own
/// clock, injected — nothing in this module reads one — so a frame
/// rendered at a chosen instant is reproducible in a test; `engine` is
/// read by the caller the same way.
///
/// **What it costs.** The log is folded once, by [`derive()`]; the frame
/// is then read off that state, so each figure after the fold is a read
/// of a ledger, never another pass over the log. A caller framing one
/// run pays for the fold once; a caller that already holds the state
/// calls [`run_frame_of`] and pays for none.
pub fn run_frame(
    run_id: &RunId,
    workflow: &Workflow,
    events: &[StoredEvent],
    prior: Option<&PriorEstimation>,
    engine: &Engine,
    now: DateTime<Utc>,
) -> RunFrame {
    run_frame_of(run_id, workflow, &derive(events), prior, engine, now)
}

/// [`run_frame`] read from a state the caller has already derived from the
/// run's own log: `state` is `derive(events)` for the events `run_frame`
/// would have been given, so both return the same frame.
pub fn run_frame_of(
    run_id: &RunId,
    workflow: &Workflow,
    state: &RunState,
    prior: Option<&PriorEstimation>,
    engine: &Engine,
    now: DateTime<Utc>,
) -> RunFrame {
    let stats = stats_observed_at(state, workflow, Some(now));
    let mode = state.run.mode().clone();
    let phase = phase::phase(workflow, state);
    let reading = Reading {
        stats: stats
            .nodes
            .iter()
            .map(|stat| (&stat.node_id, stat))
            .collect(),
        people: &stats.people_by_node,
        included: mode_included_nodes(workflow, &mode),
        left_out: state.run.left_out(),
        state,
        now,
        open: matches!(
            phase,
            RunPhase::Created | RunPhase::Running | RunPhase::Waiting { .. }
        ),
        engine,
    };
    let nodes: Vec<NodeFrame> = workflow
        .iter_nodes_with_group()
        .map(|(node, group)| reading.node(node, group))
        .collect();

    let blocking_findings = crate::dedup_findings(&state.unsettled_findings())
        .iter()
        .filter(|finding| finding.severity == FindingSeverity::Blocking)
        .count();
    RunFrame {
        run_id: run_id.clone(),
        workflow: workflow.name.clone(),
        mode: mode.clone(),
        standing: run_standing(&phase, &nodes, blocking_findings, engine),
        phase,
        elapsed: stats.wall_clock,
        flow: flow_counter(&nodes, &mode),
        tasks: task_counter(state),
        reroutes: state
            .nodes
            .values()
            .map(|record| record.reroutes as usize)
            .sum(),
        tokens: state.spend().so_far(),
        prior: prior.cloned(),
        nodes,
        children: state.children.links().to_vec(),
        degraded: state.degradations.all().to_vec(),
        unknown_kinds: unknown_kind_counts(state),
        blocking_findings,
    }
}

/// The word the run is called by, decided once from its phase, the words
/// of its nodes, the findings that still block it, and what the registry
/// of the process driving it says.
///
/// A run whose log says it moves while the process its registry names is
/// gone is `stalled`; one whose process is asking a person at its
/// terminal needs them. A paused run a person stopped is `stopped`, one
/// with a node that needs a person needs them, and one held only by a
/// limit's reset is `waiting`, which a clock ends. Only a dead process is
/// proof of a stall: no registry at all is also what a run looks like for
/// the instant it is handed to another process.
pub fn run_standing(
    phase: &RunPhase,
    nodes: &[NodeFrame],
    blocking_findings: usize,
    engine: &Engine,
) -> Standing {
    match phase {
        RunPhase::Created | RunPhase::Running if engine.liveness == EngineLiveness::Dead => {
            Standing::Stalled
        }
        RunPhase::Created | RunPhase::Running if engine.prompt.is_some() => Standing::NeedsYou,
        RunPhase::Created => Standing::Created,
        RunPhase::Running => Standing::Running,
        RunPhase::Waiting { on } => match on.reason() {
            Some(reason) if reason.by_a_person() => Standing::Stopped,
            _ if nodes.iter().any(|node| node.standing == Standing::NeedsYou) => Standing::NeedsYou,
            Some(reason) if reason.reset_wait().is_some() => Standing::Waiting,
            _ => Standing::NeedsYou,
        },
        RunPhase::Finished if blocking_findings > 0 => Standing::Reported,
        RunPhase::Finished => Standing::Finished,
        RunPhase::Failed { .. } => Standing::Failed,
        RunPhase::Cancelled => Standing::Stopped,
        RunPhase::Promoted { .. } => Standing::Promoted,
        RunPhase::Broken { .. } => Standing::Broken,
    }
}

/// The flow counter, folded from the very node frames a surface shows
/// beside it, so the count and the list under it cannot disagree: each
/// node is counted under the word its row is drawn with.
fn flow_counter(nodes: &[NodeFrame], mode: &ModeName) -> Counter {
    let mut counter = Counter::default();
    let mut by: BTreeMap<Standing, usize> = BTreeMap::new();
    for node in nodes {
        match &node.state {
            NodeStanding::Skipped => counter.skipped += 1,
            NodeStanding::LeftOut(_) => counter.left_out += 1,
            _ => {
                *by.entry(node.standing).or_default() += 1;
                counter.total += 1;
            }
        }
    }
    counter.by = by.into_iter().collect();
    counter.skipped_by = (counter.skipped > 0).then(|| mode.clone());
    counter
}

/// The ledger's counter, or `None` until a task is registered: a run
/// with no ledger reports no ledger, never a fabricated `0/0`.
fn task_counter(state: &RunState) -> Option<TaskCounter> {
    if state.tasks.is_empty() {
        return None;
    }
    let mut counter = TaskCounter {
        total: state.tasks.len(),
        ..TaskCounter::default()
    };
    for status in state.tasks.statuses() {
        match status {
            TaskStatus::Done => counter.done += 1,
            TaskStatus::Failed => counter.failed += 1,
            TaskStatus::Running => counter.running += 1,
            TaskStatus::Blocked => counter.blocked += 1,
            TaskStatus::Pending | TaskStatus::Ready => counter.to_go += 1,
        }
    }
    Some(counter)
}

/// The top-level node ids a run's mode schedules, as
/// [`mode_included_nodes`] answers it: `None` when nothing narrows the
/// graph at all.
type Included = Option<HashSet<NodeId>>;
