//! `yunta stats`: every number here is derived
//! from the event log alone — never estimated, never trusting an agent's
//! own report (the same guarantee [`crate::progress`] and
//! [`crate::replay`] already give). **CPTV and its companions** live
//! here — one run, computed from that run's own log; the comparison
//! across a workflow's past runs is [`crate::history`]'s. Presentation
//! (terminal bars, sparklines, `--json`) is the CLI's job; this module
//! only derives numbers.
//!
//! A run is derived **as of an instant**: `compute_run_stats` reads a log
//! as of its own last event, `compute_run_stats_at` reads it at a caller's
//! `now`. The second is what a run in progress needs — an attempt that
//! has not closed has no duration in the log, and the run's clock would
//! otherwise stop at the last event any node happened to write.
//!
//! **CPTV is the headline metric because it optimizes what matters:**
//! not minimizing tokens — a cheap run that verifies
//! nothing is the most expensive kind there is — but the cost of each
//! unit of *demonstrated* work. Tokens are the unit, always; a currency
//! estimate is an optional, additive line the CLI prints only when
//! `pricing:` is configured — this module never converts to currency
//! itself, so it can never be the thing that invents a number
//! ("sin `pricing:` declarado, todo se expresa en tokens y nada se
//! inventa").
//!
//! **Rework rate** is tokens spent on attempts beyond the first — a
//! node's `attempt` field (`NodeStartedPayload`) already distinguishes a
//! first try from a retry or a re-route's correction attempt, so no new
//! bookkeeping is needed to tell them apart.
//!
//! **Blocked wall-clock** (the "blocked fraction" a workflow author
//! watches for) is a *best-effort* derived
//! signal, not a scheduler replay: a node's `ready_at` is the latest
//! terminal timestamp among its own `depends_on` (or the run's first
//! event, for a node with none), and "blocked" is the gap between that
//! and when the node actually started — time spent waiting on a
//! scheduler slot (`max_parallel_nodes`) rather than on a dependency.
//! `parallel` group join semantics aren't simulated here; a child node's
//! `ready_at` is only as accurate as its own declared `depends_on`.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

use yunta_core::events::{EventPayload, StoredEvent, TaskStatus, TokenUsage};
use yunta_core::{Node, NodeId, RunnerName, Workflow};

use crate::replay::{derive, unknown_kind_counts, RunState, UnknownKindCount};

/// One node's contribution to a run's stats — declaration order (`parallel`
/// children flattened in place, same convention `crate::progress` uses).
#[derive(Debug, Clone, PartialEq)]
pub struct NodeStat {
    pub node_id: NodeId,
    /// The runner the node resolved through (`runner_resolved.runner`) —
    /// `None` only for a node that never reached that point (fails before
    /// its runner resolves).
    pub runner: Option<RunnerName>,
    /// Tokens across every attempt, first and retries alike.
    pub tokens: TokenUsage,
    /// The highest `attempt` number this node reached — `1` for a node
    /// that succeeded on its first try, never retried.
    pub attempts: u32,
    /// Sum of (terminal timestamp − start timestamp) across every attempt
    /// that closed — time actually spent running, not waiting.
    pub active: Duration,
    /// How long the attempt still open at the observation instant has
    /// been running. `None` when the node has no open attempt there —
    /// and always `None` from [`compute_run_stats`], which observes a run
    /// as of its own last event and so has no later instant to measure
    /// an open attempt against.
    ///
    /// Open means the log has not closed it, which includes an attempt
    /// parked on a gate: a node waiting on a human is inside its attempt,
    /// and its elapsed keeps growing. What it is doing there is the
    /// node's state ([`crate::NodeState`]), not this figure.
    pub open_attempt: Option<Duration>,
    /// Time between the node becoming ready (its dependencies' last
    /// terminal event, or the run's start for a root node) and its first
    /// `node_started` — see this module's own doc comment on why this is
    /// best-effort, not a scheduler replay.
    pub blocked: Duration,
}

impl NodeStat {
    /// Time this node has spent running as of the observation instant:
    /// every attempt that closed, plus the one still open.
    pub fn active_so_far(&self) -> Duration {
        self.active + self.open_attempt.unwrap_or(Duration::ZERO)
    }

    /// This node's own wall-clock: blocked, then active, back to back —
    /// the attempt still open included.
    pub fn wall_clock(&self) -> Duration {
        self.blocked + self.active_so_far()
    }

    /// `None` when there's no wall-clock to divide by yet (a node with
    /// zero recorded duration on either side) — never a manufactured 0%.
    pub fn blocked_fraction(&self) -> Option<f64> {
        let total = self.wall_clock();
        if total.is_zero() {
            return None;
        }
        Some(self.blocked.as_secs_f64() / total.as_secs_f64())
    }
}

/// One run's derived stats — everything `yunta stats <run_id>`
/// shows.
#[derive(Debug, Clone, PartialEq)]
pub struct RunStats {
    /// `None` until at least one task is `done` — never a made-up number
    /// (the definition is total run tokens / tasks done).
    pub cptv: Option<f64>,
    /// Tokens spent on attempts beyond the first, over total tokens.
    /// `None` when the run spent no tokens at all.
    pub rework_rate: Option<f64>,
    /// Cached input tokens over total input tokens. `None` when no
    /// adapter in this run ever reported a cache figure (`Usage`'s
    /// `cached_input_tokens` is an optional extension) — distinct
    /// from `Some(0.0)`, which means it reported and the answer was zero.
    pub cache_rate: Option<f64>,
    pub total_tokens: TokenUsage,
    pub tasks_total: usize,
    pub tasks_done: usize,
    /// How long the run has been going: from its first event to the
    /// observation instant while the log carries no `run_finished`, and
    /// to its last event once it does — a finished run's clock stops
    /// with it, and a run nobody is observing has only its log to
    /// measure against. `None` for an empty log; never a prediction of a
    /// total.
    pub wall_clock: Option<Duration>,
    /// Every node that reached at least one `node_started`, in the
    /// workflow's own declaration order.
    pub nodes: Vec<NodeStat>,
    /// Events this binary could not interpret, by kind.
    pub unknown_kinds: Vec<UnknownKindCount>,
}

impl RunStats {
    /// Tokens grouped by resolved role, node order broken and re-grouped
    /// — the "per role" half of "cost per node, per role, and per
    /// mode" (mode is a whole-run property today, since `modes:` isn't
    /// implemented yet — nothing to break out per role *and* per mode
    /// within one run until it is; `--workflow`'s history view is where
    /// mode comparison lives).
    pub fn tokens_by_runner(&self) -> Vec<(RunnerName, TokenUsage)> {
        let mut by_runner: Vec<(RunnerName, TokenUsage)> = Vec::new();
        for node in &self.nodes {
            let Some(runner) = &node.runner else { continue };
            match by_runner.iter_mut().find(|(r, _)| r == runner) {
                Some((_, tokens)) => *tokens += node.tokens,
                None => by_runner.push((runner.clone(), node.tokens)),
            }
        }
        by_runner
    }
}

/// CPTV: total run tokens / tasks done. The one number every
/// `RunFinished.metrics` and `RunStats` alike report, computed once here
/// so the two call sites can never disagree.
pub fn cptv(state: &RunState) -> Option<f64> {
    let done = state
        .tasks
        .values()
        .filter(|status| matches!(status, TaskStatus::Done))
        .count();
    if done == 0 {
        return None;
    }
    Some(state.total_tokens.total() as f64 / done as f64)
}

/// Derives one run's stats from its workflow and event log alone,
/// observed as of the log's own last event. Pure: same input, same
/// output, always.
///
/// An attempt still open at that point contributes no elapsed time: with
/// no instant later than the log to measure against, how long it has
/// been running is unknown, never zero. [`compute_run_stats_at`] is this
/// same derivation given one.
pub fn compute_run_stats(workflow: &Workflow, events: &[StoredEvent]) -> RunStats {
    stats_observed_at(&derive(events), workflow, events, None)
}

/// Derives one run's stats as they stand at `now` — what a run in
/// progress needs: the attempt open at `now` reports how long it has
/// been running ([`NodeStat::open_attempt`]), and a run with no
/// `run_finished` measures its wall-clock to `now`, so a node that has
/// been silent for minutes cannot freeze the run's clock at its last
/// event. `now` is the caller's own instant, injected: this module never
/// reads a clock.
pub fn compute_run_stats_at(
    workflow: &Workflow,
    events: &[StoredEvent],
    now: DateTime<Utc>,
) -> RunStats {
    stats_observed_at(&derive(events), workflow, events, Some(now))
}

/// The one derivation behind [`compute_run_stats`] and
/// [`compute_run_stats_at`]: `observed_at` is the instant the run is
/// looked at, or `None` to look at it as of its own last event.
///
/// `state` is the log already replayed. A caller that reads several
/// things off one log — a frame reads its stats, its phase and its
/// tokens — replays it once and hands the answer down, rather than
/// paying for the same fold again per reader.
pub(crate) fn stats_observed_at(
    state: &RunState,
    workflow: &Workflow,
    events: &[StoredEvent],
    observed_at: Option<DateTime<Utc>>,
) -> RunStats {
    let flat: Vec<&Node> = workflow.iter_nodes().collect();
    let run_start = events.first().map(|e| e.timestamp);
    let walk = walk_attempts(events);

    let total = state.total_tokens.total();
    let rework_total = walk.rework_tokens.total();
    let rework_rate = if total == 0 {
        None
    } else {
        Some(rework_total as f64 / total as f64)
    };
    // A rate needs input tokens to divide by; without them the answer is
    // undefined, distinct from `Some(0.0)` (an adapter reported and cached
    // nothing) — never a denominator invented with `.max(1)`.
    let cache_rate = match state.total_tokens.cached {
        Some(cached) if state.total_tokens.input > 0 => {
            Some(cached as f64 / state.total_tokens.input as f64)
        }
        _ => None,
    };

    RunStats {
        unknown_kinds: unknown_kind_counts(state),
        cptv: cptv(state),
        rework_rate,
        cache_rate,
        total_tokens: state.total_tokens,
        tasks_total: state.tasks.len(),
        tasks_done: state
            .tasks
            .values()
            .filter(|s| matches!(s, TaskStatus::Done))
            .count(),
        wall_clock: run_start
            .zip(measured_until(events, observed_at))
            .map(|(start, until)| interval(start, until)),
        nodes: node_stats(&flat, &walk, run_start, observed_at),
    }
}

/// The time from `from` to `to` — how every duration this module reports
/// reads an interval. [`Duration::ZERO`] when `to` is the earlier of the
/// two: nothing a log describes has run for a negative time, and a
/// caller's clock sitting behind the log still gets a measured answer
/// rather than one that reads as missing.
fn interval(from: DateTime<Utc>, to: DateTime<Utc>) -> Duration {
    (to - from).to_std().unwrap_or(Duration::ZERO)
}

/// The instant a run's wall-clock is measured to: `observed_at` while
/// the run has not written its `run_finished` and a caller supplied one,
/// the log's last event otherwise. A finished run's clock stops where it
/// stopped, whoever looks at it and whenever.
fn measured_until(
    events: &[StoredEvent],
    observed_at: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    let finished = events
        .iter()
        .rev()
        .any(|e| matches!(e.payload(), Some(EventPayload::RunFinished(_))));
    match observed_at {
        Some(now) if !finished => Some(now),
        _ => events.last().map(|e| e.timestamp),
    }
}

/// Every node that reached at least one `node_started`, in the
/// workflow's own declaration order, with the elapsed of the attempt
/// open at `observed_at` on top of what the walk measured.
fn node_stats(
    flat: &[&Node],
    walk: &AttemptWalk,
    run_start: Option<DateTime<Utc>>,
    observed_at: Option<DateTime<Utc>>,
) -> Vec<NodeStat> {
    let depends_on: HashMap<&NodeId, &[NodeId]> = flat
        .iter()
        .map(|n| (&n.id, n.depends_on.as_slice()))
        .collect();

    let mut nodes = Vec::new();
    for node in flat {
        let Some(&attempts) = walk.max_attempt.get(&node.id) else {
            continue; // never started — nothing to report.
        };
        let deps = depends_on.get(&node.id).copied().unwrap_or(&[]);
        nodes.push(NodeStat {
            node_id: node.id.clone(),
            runner: walk.runner.get(&node.id).cloned(),
            tokens: walk.node_tokens.get(&node.id).copied().unwrap_or_default(),
            attempts,
            active: walk
                .node_active
                .get(&node.id)
                .copied()
                .unwrap_or(Duration::ZERO),
            open_attempt: observed_at.and_then(|now| {
                walk.open
                    .get(&node.id)
                    .map(|open| interval(open.started_at, now))
            }),
            blocked: walk.blocked_before_start(&node.id, deps, run_start),
        });
    }
    nodes
}

/// Everything one pass over a run's log measures per node: the attempt
/// each node has open, what its closed attempts spent and how long they
/// ran, when each node first started, the highest attempt it reached and
/// the runner that resolved for it. Grouped because the walk touches
/// them together, one event at a time.
#[derive(Default)]
struct AttemptWalk {
    open: HashMap<NodeId, OpenAttempt>,
    node_tokens: HashMap<NodeId, TokenUsage>,
    rework_tokens: TokenUsage,
    node_active: HashMap<NodeId, Duration>,
    last_terminal: HashMap<NodeId, DateTime<Utc>>,
    first_started: HashMap<NodeId, DateTime<Utc>>,
    max_attempt: HashMap<NodeId, u32>,
    runner: HashMap<NodeId, RunnerName>,
}

impl AttemptWalk {
    /// Folds a `node_started` in: it opens this node's attempt, and the
    /// node keeps its earliest start and the highest attempt number the
    /// log has reached for it.
    fn start_attempt(&mut self, node_id: &NodeId, at: DateTime<Utc>, attempt: u32) {
        self.first_started.entry(node_id.clone()).or_insert(at);
        self.max_attempt
            .entry(node_id.clone())
            .and_modify(|a| *a = (*a).max(attempt))
            .or_insert(attempt);
        self.open.insert(
            node_id.clone(),
            OpenAttempt {
                attempt,
                started_at: at,
            },
        );
    }

    /// Folds a `node_finished`/`node_failed` in: its tokens are the
    /// node's, and a retry's are rework on top. A terminal that names no
    /// node closes nothing.
    fn close_attempt(&mut self, node_id: &Option<NodeId>, at: DateTime<Utc>, tokens: TokenUsage) {
        let Some(node_id) = node_id else { return };
        self.last_terminal.insert(node_id.clone(), at);
        let entry = self.node_tokens.entry(node_id.clone()).or_default();
        *entry += tokens;
        let Some(opened) = self.open.remove(node_id) else {
            return;
        };
        let duration = interval(opened.started_at, at);
        *self.node_active.entry(node_id.clone()).or_default() += duration;
        if opened.attempt > 1 {
            self.rework_tokens += tokens;
        }
    }

    /// How long a node waited between becoming ready — its dependencies'
    /// last terminal, or the run's start for a root node — and its own
    /// first `node_started`. [`Duration::ZERO`] for a node that started
    /// as soon as it was ready, and for one the log gives no instant to
    /// measure between.
    fn blocked_before_start(
        &self,
        node_id: &NodeId,
        deps: &[NodeId],
        run_start: Option<DateTime<Utc>>,
    ) -> Duration {
        let ready_at = if deps.is_empty() {
            run_start
        } else {
            deps.iter()
                .filter_map(|dep| self.last_terminal.get(dep))
                .max()
                .copied()
                .or(run_start)
        };
        match (ready_at, self.first_started.get(node_id)) {
            (Some(ready), Some(started)) => interval(ready, *started),
            _ => Duration::ZERO,
        }
    }
}

/// One pass over the log, folding every event that moves a node's
/// attempts into [`AttemptWalk`]. Everything a node stat needs comes
/// from here, so the log is walked once however many nodes the workflow
/// declares.
fn walk_attempts(events: &[StoredEvent]) -> AttemptWalk {
    let mut walk = AttemptWalk::default();
    for event in events {
        match event.payload() {
            Some(EventPayload::NodeStarted(p)) => {
                if let Some(node_id) = &event.node_id {
                    walk.start_attempt(node_id, event.timestamp, p.attempt);
                }
            }
            Some(EventPayload::NodeFinished(p)) => {
                walk.close_attempt(&event.node_id, event.timestamp, p.tokens_used);
            }
            Some(EventPayload::NodeFailed(p)) => {
                walk.close_attempt(&event.node_id, event.timestamp, p.tokens_used);
            }
            Some(EventPayload::RunnerResolved(p)) => {
                if let Some(node_id) = &event.node_id {
                    walk.runner.insert(node_id.clone(), p.runner.clone());
                }
            }
            _ => {}
        }
    }
    walk
}

struct OpenAttempt {
    attempt: u32,
    started_at: DateTime<Utc>,
}

/// The middle value of an already-sorted-ascending slice, averaging the two
/// central samples on an even count — the true median, not the nearest-rank
/// one a percentile picks. `None` for an empty slice: a median of nothing is
/// not zero. This is the one median the whole workspace shares.
pub fn median(sorted: &[f64]) -> Option<f64> {
    let n = sorted.len();
    match (n, n % 2) {
        (0, _) => None,
        (_, 1) => sorted.get(n / 2).copied(),
        _ => match (sorted.get(n / 2 - 1), sorted.get(n / 2)) {
            (Some(lo), Some(hi)) => Some((lo + hi) / 2.0),
            _ => None,
        },
    }
}
