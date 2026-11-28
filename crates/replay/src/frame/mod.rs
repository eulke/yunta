//! [`RunFrame`] — one derived snapshot of a run, ready for any surface
//! to present, and [`RunTimes`], what the clock adds to it at an instant.
//!
//! The same family as [`RunState::replay`] and
//! [`RunStats::at`](crate::stats::RunStats::at): a pure
//! function of the run's own event log, the workflow its manifest
//! froze, the history its caller read and what the registry says of the
//! process driving it. Nothing here reads a clock, touches storage, or
//! knows what the answer gets drawn with — a terminal region, a `--json`
//! document and a control-plane tool read the same frame, so no two of
//! them can disagree about what a run's progress means.
//!
//! **The frame is what the log says at its head; the times are what the
//! clock adds.** No field of a frame moves between two events while the
//! process driving the run reads the same: a reader that follows a run
//! derives its frame once for each head and each reading of that process,
//! and keeps it. What moves with the clock — how long the run has gone and
//! where its time went, how long it has stood, how long a node has worked,
//! gone quiet or kept a person — is [`RunTimes`]'s, worked out from the same
//! state at each instant a surface draws (`times`). Not one frame read at an
//! instant: kept whole, a frame read at one instant would go stale at the
//! next, and derived at every instant it would fold the reading of every
//! node, its calls among them, at every draw.
//!
//! **Composition is a tree, never an average**: a child run's events live
//! under their own `run_id`, in a log this frame never opens. A parent
//! carries the [`ChildLink`]s its own log recorded, and the caller frames
//! each child from the child's own log and manifest.

mod actions;
mod chronicle;
mod node;
mod phase;
#[cfg(feature = "testkit")]
mod sample;
mod times;

use std::collections::{BTreeMap, HashSet};

use surco::words::Standing;

use crate::history::PriorEstimation;
use crate::modes;
use crate::registry::Engine;
use crate::RunState;
use yunta_core::caution::Caution;
use yunta_core::events::{
    ChildLink, Degradation, EngineFact, Failure, NodeOutcome, NodeState, PauseReason, TaskStatus,
    TokenUsage, UnknownKindCount,
};
use yunta_core::{ModeName, NodeId, RunId, Workflow, WorkflowName};

use node::Reading;

pub use actions::{Action, Effect};
pub use chronicle::{chronicle, Happening, Moment};
pub use node::{node_word, Declares, GateAsks, NodeFrame, NodeStanding, RunningTask};

pub use phase::{RunPhase, WaitingOn};
pub use times::{NodeTimes, RunTimes};

/// The DAG's nodes split by where each stands: the word its row says.
///
/// **No method divides finished by `total`.** A percentage lies the moment
/// a reroute grows the denominator, so leaving the division out of the
/// type beats trusting every surface to remember.
///
/// **Nothing here drops in silence.** A rerouted node runs again, so a
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

/// A run as its log stands at its head: every field a pure function of
/// that run's log, the workflow frozen in its manifest, the history its
/// caller read and what its registry says of the process driving it. No
/// field moves with the clock: what does is [`RunTimes`]'s.
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
    /// the history ([`PriorEstimation::of`]) —
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
impl RunFrame {
    /// One run's frame: from its replayed state, the workflow its manifest
    /// froze, the history the caller read, and what its registry says about
    /// the process driving it.
    ///
    /// Pure: same inputs, same frame, always. `engine` is read by the
    /// caller, as the instant [`RunTimes::of`] measures to is: nothing in
    /// this module reads a process or a clock, so a frame is reproducible in
    /// a test.
    ///
    /// **What it costs.** The log is folded once, by [`RunState::replay`],
    /// before this is called; the frame is then read off that state, so
    /// each figure is a read of a ledger, never another pass over the log.
    /// It reads every node's record, the calls of the attempt a node runs
    /// among them, so a reader that draws a run again and again derives its
    /// frame once for each head and each reading of `engine`, and works out
    /// only its [`RunTimes`] at each draw.
    pub fn of(
        run_id: &RunId,
        workflow: &Workflow,
        state: &RunState,
        prior: Option<&PriorEstimation>,
        engine: &Engine,
    ) -> RunFrame {
        let mode = state.run.mode().clone();
        let phase = phase::phase(workflow, state);
        let reading = Reading {
            included: modes::included(workflow, &mode),
            left_out: state.run.left_out(),
            state,
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

        let blocking_findings = state.findings_that_block().len();
        RunFrame {
            run_id: run_id.clone(),
            workflow: workflow.name.clone(),
            mode: mode.clone(),
            standing: run_standing(&phase, &nodes, blocking_findings, engine),
            phase,
            flow: flow_counter(&nodes, &mode),
            tasks: task_counter(state),
            reroutes: state.nodes.reroutes(),
            tokens: state.spend().so_far(),
            prior: prior.cloned(),
            nodes,
            children: state.children.links().to_vec(),
            degraded: state.degradations.all().to_vec(),
            unknown_kinds: state.unknown_kinds(),
            blocking_findings,
        }
    }

    /// Whether the run still moves as this frame reads it: created or
    /// running, under an engine that lives. A run its log ended, parked or
    /// promoted does not move, nor does one whose engine is gone (`stalled`):
    /// what a follower of the run, in any interface, stops at.
    pub fn moves(&self) -> bool {
        matches!(self.phase, RunPhase::Created | RunPhase::Running)
            && self.standing != Standing::Stalled
    }

    /// The node the run waits on a person at: the first, in declaration order, whose own
    /// standing says it needs them; else the node of its own the pause it stands at names,
    /// where that pause waits on a person, a node whose log says it failed and whose failure
    /// the run is parked on for a decision among them. None where nothing waits on a person,
    /// or where what does names no node: a cap, no room, a stop a person made. Every surface
    /// reads where a run waits on its person here, the node a pointer goes to and the mark
    /// a node's row carries alike.
    pub fn waits_on_a_person_at(&self) -> Option<&NodeId> {
        let needs_you = self
            .nodes
            .iter()
            .find(|node| node.standing == Standing::NeedsYou);
        if let Some(node) = needs_you {
            return Some(&node.id);
        }
        let waiting_on = self.phase.waiting_on()?;
        let asks = waiting_on.reason()?.waits_on_a_person();
        let named = waiting_on.names().filter(|_| asks)?;
        let held = self.nodes.iter().any(|node| node.id == *named);
        held.then_some(named)
    }

    /// The nodes that stand failed, each with its failure, in declaration
    /// order. A node whose log says it failed can stand otherwise: one a
    /// reset holds is `waiting`, and the pause says what holds it; one a
    /// reroute corrects is `rerouted`. Neither is listed.
    pub fn failed(&self) -> impl Iterator<Item = (&NodeFrame, &Failure)> {
        self.nodes.iter().filter_map(|node| match &node.state {
            NodeStanding::Reached(NodeState::Failed { failure, .. })
                if node.standing == Standing::Failed =>
            {
                Some((node, failure))
            }
            _ => None,
        })
    }

    /// Each pull request the run opened, in the order its nodes are
    /// declared: a page and the run's document read them here, so both say
    /// the same.
    pub fn pull_requests(&self) -> impl Iterator<Item = PullRequest<'_>> {
        self.nodes.iter().filter_map(|node| match &node.state {
            NodeStanding::Reached(NodeState::Finished {
                outcome: NodeOutcome::Fact(EngineFact::PullRequestOpened { number, url }),
                ..
            }) => Some(PullRequest {
                number: *number,
                url,
            }),
            _ => None,
        })
    }

    /// How many artifacts the run's nodes wrote, across every node.
    pub fn artifacts_written(&self) -> usize {
        self.nodes.iter().map(|node| node.artifacts.len()).sum()
    }

    /// The cautions the run raises: a run parked on its token budget says
    /// what the declared cap means against what this workflow's past runs
    /// spent; a run parked on anything else raises none.
    pub fn cautions(&self) -> Vec<Caution> {
        match &self.phase {
            RunPhase::Waiting {
                on:
                    WaitingOn::Run {
                        reason: PauseReason::BudgetExhausted { cap, .. },
                    },
            } => crate::history::budget_caution(Some(*cap), self.prior.as_ref())
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// A pull request a run opened: its number on the forge, and where it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PullRequest<'a> {
    pub number: u64,
    pub url: &'a str,
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
        RunPhase::Created | RunPhase::Running if *engine == Engine::Dead => Standing::Stalled,
        RunPhase::Created | RunPhase::Running if engine.prompt().is_some() => Standing::NeedsYou,
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
/// [`modes::included`] answers it: `None` when nothing narrows the
/// graph at all.
type Included = Option<HashSet<NodeId>>;
