//! `yunta stats`: every number here is derived
//! from the event log alone — never estimated, never trusting an agent's
//! own report (the same guarantee [`crate::progress`] and
//! [`crate::replay`] already give). **CPTV and its companions** are read
//! here off one run's derived state, and the arithmetic that makes CPTV is
//! `RunMetrics::of` in core, so a run's close and its stats can never
//! disagree; the comparison across a workflow's past runs is
//! [`crate::history`]'s. Presentation
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
//! (with no `pricing:` declared, everything is expressed in tokens and
//! nothing is invented).
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

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

use yunta_core::events::{Spend, StoredEvent, Suspensions, TokenUsage};
use yunta_core::rail::timing::Histogram;
use yunta_core::rail::RailEvent;
use yunta_core::{ModelName, Node, NodeId, RunnerName, Workflow};

use crate::replay::{derive, unknown_kind_counts, RunState, TimeSpent, UnknownKindCount};

/// One node's contribution to a run's stats — declaration order (`parallel`
/// children flattened in place, same convention `crate::progress` uses).
#[derive(Debug, Clone, PartialEq)]
pub struct NodeStat {
    pub node_id: NodeId,
    /// The runner the node resolved through (`runner_resolved.runner`) —
    /// `None` only for a node that never reached that point (fails before
    /// its runner resolves).
    pub runner: Option<RunnerName>,
    /// Tokens across every attempt, first and retries alike: what the
    /// attempts settled, and what the open one has reported so far.
    pub tokens: TokenUsage,
    /// The highest `attempt` number this node reached — `1` for a node
    /// that succeeded on its first try, never retried.
    pub attempts: u32,
    /// Working time across the node's attempts: each stretch of work, from
    /// a start or an answer to the question or terminal that closed it.
    /// Time the node spent waiting on a person is left out, and so is the
    /// stretch an attempt has open at the observation instant.
    pub active: Duration,
    /// How long the stretch of work still open at the observation instant
    /// has been running. `None` when the node has no open attempt there,
    /// and while it waits on a person — a question, or a gate published
    /// to a forge: that wait is the node's state ([`crate::NodeState`]),
    /// not work. Always `None` from [`compute_run_stats`], which observes
    /// a run as of its own last event and so has no later instant to
    /// measure an open attempt against.
    pub open_attempt: Option<Duration>,
    /// Time between the node becoming ready (its dependencies' last
    /// terminal event, or the run's start for a root node) and its first
    /// `node_started` — see this module's own doc comment on why this is
    /// best-effort, not a scheduler replay.
    pub blocked: Duration,
    /// The models the node's sessions ran on, as their CLIs reported them,
    /// never the model the run asked for.
    pub models: Vec<ModelName>,
}

impl NodeStat {
    /// Time this node has spent working as of the observation instant:
    /// every stretch of work that closed, plus the one still open.
    pub fn active_so_far(&self) -> Duration {
        self.active + self.open_attempt.unwrap_or(Duration::ZERO)
    }

    /// This node's own wall-clock: blocked, then active, back to back —
    /// the attempt still open included.
    pub fn wall_clock(&self) -> Duration {
        self.blocked + self.active_so_far()
    }

    /// `None` when there is no wall-clock to divide by (a node with zero
    /// recorded duration on either side) — never a manufactured 0%.
    pub fn blocked_fraction(&self) -> Option<f64> {
        let total = self.wall_clock();
        if total.is_zero() {
            return None;
        }
        Some(self.blocked.as_secs_f64() / total.as_secs_f64())
    }
}

/// Whole documents sessions offered, by what the engine answered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Submissions {
    pub accepted: u64,
    pub refused: u64,
}

/// Finding calls a log carries, by what the engine answered: the
/// accepted forms — a node's own finding posted, updated or withdrawn,
/// and another node's answered — and the calls it turned down.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct FindingActivity {
    pub posted: u64,
    pub updated: u64,
    pub withdrawn: u64,
    pub refused: u64,
    pub answered: u64,
    pub proved: u64,
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
    /// What the run spent, settled and in flight. Every total here reads
    /// it: `total_tokens` is its [`Spend::so_far`].
    pub spend: Spend,
    /// The run's whole spend as one figure: `spend.so_far()`.
    pub total_tokens: TokenUsage,
    pub tasks_total: usize,
    pub tasks_done: usize,
    /// How long the run has been going: from its first event to the
    /// observation instant while the run has neither finished nor broken,
    /// and to its last applied event once it has — a finished run's clock
    /// stops with its close, and a log that breaks stops at the break. The
    /// time its host was suspended is left out: this is how long the run
    /// had a machine to work on. `None` for an empty log; never a
    /// prediction of a total.
    pub wall_clock: Option<Duration>,
    /// How long the host was suspended inside that same window — what
    /// [`RunStats::wall_clock`] and every node's durations leave out.
    pub asleep: Duration,
    /// That same wall-clock, by what it went to.
    pub time: TimeSpent,
    /// Every node that reached at least one `node_started`, in the
    /// workflow's own declaration order.
    pub nodes: Vec<NodeStat>,
    /// Events this binary could not interpret, by kind.
    pub unknown_kinds: Vec<UnknownKindCount>,
    /// Every document submitted across the run, by verdict.
    pub artifact_submissions: Submissions,
    /// The same, for each node that submitted at least one.
    pub submissions_by_node: BTreeMap<NodeId, Submissions>,
    /// Finding events as the log carries them — a session's own calls,
    /// the engine's posts (a gate escalation, a scope request) and a
    /// `kind: workflow` node's re-emission of its child run's findings
    /// all count the same.
    pub findings: FindingActivity,
    /// The same, for each node whose findings the log carries.
    pub findings_by_node: BTreeMap<NodeId, FindingActivity>,
    /// How many findings stand now: the fold over the whole log, where
    /// an update replaces and a withdrawal removes — never a count of
    /// `finding_posted` events.
    pub findings_effective: u64,
    /// How many of those a proof or a person settled.
    pub findings_settled: u64,
    /// How long the rail's calls took at each moment, merged over every
    /// session of the run; empty when no rail call was made.
    pub rails: BTreeMap<RailEvent, Histogram>,
}

impl RunStats {
    /// Tokens grouped by resolved role, node order broken and re-grouped
    /// — the "per role" half of "cost per node, per role, and per
    /// mode". Mode is a whole-run property: a run is created in one
    /// mode, so there is nothing to break out per role *and* per mode
    /// within one run, and `--workflow`'s history view is where mode
    /// comparison lives.
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

/// Derives one run's stats from its workflow and event log alone,
/// observed as of the log's own last event. Pure: same input, same
/// output, always.
///
/// An attempt still open at that point contributes no elapsed time: with
/// no instant later than the log to measure against, how long it has
/// been running is unknown, never zero. [`compute_run_stats_at`] is this
/// same derivation given one.
pub fn compute_run_stats(workflow: &Workflow, events: &[StoredEvent]) -> RunStats {
    compute_run_stats_of(workflow, &derive(events))
}

/// [`compute_run_stats`] read from a state the caller has already derived
/// from the run's own log: `state` is `derive(events)` for the events
/// `compute_run_stats` would have been given, so both return the same
/// stats. A caller that reads other things off the same state folds the
/// log once.
pub fn compute_run_stats_of(workflow: &Workflow, state: &RunState) -> RunStats {
    stats_observed_at(state, workflow, None)
}

/// Derives one run's stats as they stand at `now` — what a run in
/// progress needs: the attempt open at `now` reports how long it has
/// been working ([`NodeStat::open_attempt`]), and a run that has not
/// finished measures its wall-clock to `now`, so a node that has been
/// silent for minutes cannot freeze the run's clock at its last event.
/// `now` is the caller's own instant, injected: this module never reads
/// a clock.
pub fn compute_run_stats_at(
    workflow: &Workflow,
    events: &[StoredEvent],
    now: DateTime<Utc>,
) -> RunStats {
    stats_observed_at(&derive(events), workflow, Some(now))
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
    observed_at: Option<DateTime<Utc>>,
) -> RunStats {
    let asleep = state.run.suspensions();
    // The clock is read to the observation while the run is still going,
    // and to its last applied event once it has finished or broken: a
    // finished run's clock stops with its close, a broken one at the break.
    let until = match observed_at {
        Some(now) if !state.ended() => Some(now),
        _ => state.clock().last_at(),
    };
    let spend = state.spend();
    let total = spend.so_far();
    let activity = state.activity();
    RunStats {
        cptv: state.metrics().cptv,
        rework_rate: rework_rate(state, total),
        cache_rate: cache_rate(total),
        spend,
        total_tokens: total,
        tasks_total: state.tasks.len(),
        tasks_done: state.tasks.done(),
        wall_clock: state.elapsed(until),
        asleep: state
            .clock()
            .window(until)
            .map(|(start, end)| asleep.asleep_between(start, end))
            .unwrap_or_default(),
        time: state.time_spent(until),
        nodes: node_stats(state, workflow, asleep, observed_at),
        unknown_kinds: unknown_kind_counts(state),
        artifact_submissions: activity.submissions,
        submissions_by_node: activity.submissions_by_node.clone(),
        findings: activity.findings,
        findings_by_node: activity.findings_by_node.clone(),
        findings_effective: state.findings.effective().len() as u64,
        findings_settled: findings_settled(state),
        rails: activity.rails.clone(),
    }
}

/// Tokens spent on attempts beyond the first, over `total`. `None` when
/// the run spent nothing at all.
fn rework_rate(state: &RunState, total: TokenUsage) -> Option<f64> {
    if total.total() == 0 {
        return None;
    }
    let rework: TokenUsage = state.nodes.iter().map(|(_, record)| record.rework).sum();
    Some(rework.total() as f64 / total.total() as f64)
}

/// Cached input tokens over input tokens. A rate needs input tokens to
/// divide by; without them the answer is undefined, distinct from
/// `Some(0.0)` (an adapter reported and cached nothing) — never a
/// denominator invented with `.max(1)`.
fn cache_rate(total: TokenUsage) -> Option<f64> {
    match total.cached {
        Some(cached) if total.input > 0 => Some(cached as f64 / total.input as f64),
        _ => None,
    }
}

/// How many of the findings that stand a proof or a person settled.
fn findings_settled(state: &RunState) -> u64 {
    (state.findings.effective().len() - state.findings.unsettled().len()) as u64
}

/// Every node that reached at least one `node_started`, in the
/// workflow's own declaration order, each read off its own record.
fn node_stats(
    state: &RunState,
    workflow: &Workflow,
    asleep: &Suspensions,
    observed_at: Option<DateTime<Utc>>,
) -> Vec<NodeStat> {
    let run_start = state.clock().first_at();
    workflow
        .iter_nodes()
        .filter_map(|node| node_stat(state, node, asleep, run_start, observed_at))
        .collect()
}

/// One node's stat, when the log says it started. Everything but the open
/// attempt's elapsed is already on the node's record.
fn node_stat(
    state: &RunState,
    node: &Node,
    asleep: &Suspensions,
    run_start: Option<DateTime<Utc>>,
    observed_at: Option<DateTime<Utc>>,
) -> Option<NodeStat> {
    let record = state.nodes.get(&node.id)?;
    let started = record.first_started?;
    // Ready once every dependency has reached a terminal; a node with no
    // dependency, or with none that has, is ready at the run's start.
    let ready = node
        .depends_on
        .iter()
        .filter_map(|dep| state.nodes.get(dep)?.last_terminal_at)
        .max()
        .or(run_start);
    Some(NodeStat {
        node_id: node.id.clone(),
        runner: record
            .runner
            .as_ref()
            .map(|resolved| resolved.runner.clone()),
        tokens: record.spend.so_far(),
        attempts: record.attempts,
        active: record.active,
        open_attempt: observed_at
            .zip(record.working_since)
            .map(|(now, since)| asleep.awake_between(since, now)),
        blocked: ready.map_or(Duration::ZERO, |ready| asleep.awake_between(ready, started)),
        models: record.models.clone(),
    })
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
