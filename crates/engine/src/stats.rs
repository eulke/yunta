//! `yunta stats` (T7.5, Contrato §8.4/§8.6): every number here is derived
//! from the event log alone — never estimated, never trusting an agent's
//! own report (I20, same guarantee [`crate::progress`] and
//! [`crate::replay`] already give). Two things live here: **CPTV and its
//! companions** (§8.4 — one run, computed from that run's own log) and
//! **prior estimation** (§8.6 — a workflow's own run history, computed
//! from several runs' logs at once). Presentation (terminal bars,
//! sparklines, `--json`) is the CLI's job; this module only derives
//! numbers.
//!
//! **CPTV is the headline metric because it optimizes what matters**
//! (§8.4's own text): not minimizing tokens — a cheap run that verifies
//! nothing is the most expensive kind there is — but the cost of each
//! unit of *demonstrated* work. Tokens are the unit, always; a currency
//! estimate is an optional, additive line the CLI prints only when
//! `pricing:` is configured — this module never converts to currency
//! itself, so it can never be the thing that invents a number (§8.4:
//! "sin `pricing:` declarado, todo se expresa en tokens y nada se
//! inventa").
//!
//! **Rework rate** is tokens spent on attempts beyond the first — a
//! node's `attempt` field (`NodeStartedPayload`) already distinguishes a
//! first try from a retry or a re-route's correction attempt, so no new
//! bookkeeping is needed to tell them apart.
//!
//! **Blocked wall-clock** (the "fracción bloqueada" the plan asks for,
//! feeding A-08's own trigger condition) is a *best-effort* derived
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

use yunta_core::events::{Event, EventPayload, TaskStatus, TokenUsage};
use yunta_core::{Node, NodeId, RunId, Workflow};

use crate::replay::{derive, RunState};

/// One node's contribution to a run's stats — declaration order (`parallel`
/// children flattened in place, same convention [`crate::progress`] uses).
#[derive(Debug, Clone, PartialEq)]
pub struct NodeStat {
    pub node_id: NodeId,
    /// The role its `runner:` resolved through (`runner_resolved.role`) —
    /// `None` only for a node that never reached that point (fails before
    /// its runner resolves).
    pub role: Option<String>,
    /// Tokens across every attempt, first and retries alike.
    pub tokens: TokenUsage,
    /// The highest `attempt` number this node reached — `1` for a node
    /// that succeeded on its first try, never retried.
    pub attempts: u32,
    /// Sum of (terminal timestamp − start timestamp) across every attempt
    /// — time actually spent running, not waiting.
    pub active: Duration,
    /// Time between the node becoming ready (its dependencies' last
    /// terminal event, or the run's start for a root node) and its first
    /// `node_started` — see this module's own doc comment on why this is
    /// best-effort, not a scheduler replay.
    pub blocked: Duration,
}

impl NodeStat {
    /// This node's own wall-clock: blocked, then active, back to back.
    pub fn wall_clock(&self) -> Duration {
        self.blocked + self.active
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

/// One run's derived stats (§8.4) — everything `yunta stats <run_id>`
/// shows.
#[derive(Debug, Clone, PartialEq)]
pub struct RunStats {
    /// `None` until at least one task is `done` — never a made-up number
    /// (§8.4's own definition: total run tokens / tasks done).
    pub cptv: Option<f64>,
    /// Tokens spent on attempts beyond the first, over total tokens.
    /// `None` when the run spent no tokens at all.
    pub rework_rate: Option<f64>,
    /// Cached input tokens over total input tokens. `None` when no
    /// adapter in this run ever reported a cache figure (`Usage`'s
    /// `cached_input_tokens` is an optional extension, §8.4) — distinct
    /// from `Some(0.0)`, which means it reported and the answer was zero.
    pub cache_rate: Option<f64>,
    pub total_tokens: TokenUsage,
    pub tasks_total: usize,
    pub tasks_done: usize,
    /// Last event's timestamp minus the first's — `None` for an empty
    /// log. For a run still in progress this is elapsed time so far, not
    /// a prediction of the total.
    pub wall_clock: Option<Duration>,
    /// Every node that reached at least one `node_started`, in the
    /// workflow's own declaration order.
    pub nodes: Vec<NodeStat>,
}

impl RunStats {
    /// Tokens grouped by resolved role, node order broken and re-grouped
    /// — the "por rol" half of §8.4's "costo por nodo, por rol y por
    /// modo" (mode is a whole-run property in M-0, since `modes:` isn't
    /// implemented yet — nothing to break out per role *and* per mode
    /// within one run until it is; `--workflow`'s history view is where
    /// mode comparison lives).
    pub fn tokens_by_role(&self) -> Vec<(String, TokenUsage)> {
        let mut by_role: Vec<(String, TokenUsage)> = Vec::new();
        for node in &self.nodes {
            let Some(role) = &node.role else { continue };
            match by_role.iter_mut().find(|(r, _)| r == role) {
                Some((_, tokens)) => *tokens = sum_tokens(*tokens, node.tokens),
                None => by_role.push((role.clone(), node.tokens)),
            }
        }
        by_role
    }
}

/// CPTV (§8.4): total run tokens / tasks done. The one number every
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
    let total = state.total_tokens.input + state.total_tokens.output;
    Some(total as f64 / done as f64)
}

/// Derives one run's stats (§8.4) from its workflow and event log alone.
/// Pure: same input, same output, always.
pub fn compute_run_stats(workflow: &Workflow, events: &[Event]) -> RunStats {
    let state = derive(events);
    let flat: Vec<&Node> = workflow.iter_nodes().collect();
    let depends_on: HashMap<&NodeId, &[NodeId]> = flat
        .iter()
        .map(|n| (&n.id, n.depends_on.as_slice()))
        .collect();

    let run_start = events.first().map(|e| e.timestamp);
    let wall_clock = match (events.first(), events.last()) {
        (Some(first), Some(last)) => (last.timestamp - first.timestamp).to_std().ok(),
        _ => None,
    };

    let mut open: HashMap<NodeId, OpenAttempt> = HashMap::new();
    let mut first_started: HashMap<NodeId, DateTime<Utc>> = HashMap::new();
    let mut last_terminal: HashMap<NodeId, DateTime<Utc>> = HashMap::new();
    let mut node_tokens: HashMap<NodeId, TokenUsage> = HashMap::new();
    let mut rework_tokens = TokenUsage::default();
    let mut node_active: HashMap<NodeId, Duration> = HashMap::new();
    let mut node_max_attempt: HashMap<NodeId, u32> = HashMap::new();
    let mut node_role: HashMap<NodeId, String> = HashMap::new();

    for event in events {
        match &event.payload {
            EventPayload::NodeStarted(p) => {
                let Some(node_id) = &event.node_id else {
                    continue;
                };
                first_started
                    .entry(node_id.clone())
                    .or_insert(event.timestamp);
                node_max_attempt
                    .entry(node_id.clone())
                    .and_modify(|a| *a = (*a).max(p.attempt))
                    .or_insert(p.attempt);
                open.insert(
                    node_id.clone(),
                    OpenAttempt {
                        attempt: p.attempt,
                        started_at: event.timestamp,
                    },
                );
            }
            EventPayload::NodeFinished(p) => {
                close_attempt(
                    &event.node_id,
                    event.timestamp,
                    p.tokens_used,
                    &mut open,
                    &mut node_tokens,
                    &mut rework_tokens,
                    &mut node_active,
                    &mut last_terminal,
                );
            }
            EventPayload::NodeFailed(p) => {
                close_attempt(
                    &event.node_id,
                    event.timestamp,
                    p.tokens_used,
                    &mut open,
                    &mut node_tokens,
                    &mut rework_tokens,
                    &mut node_active,
                    &mut last_terminal,
                );
            }
            EventPayload::RunnerResolved(p) => {
                if let Some(node_id) = &event.node_id {
                    node_role.insert(node_id.clone(), p.role.clone());
                }
            }
            _ => {}
        }
    }

    let mut nodes = Vec::new();
    for node in &flat {
        let Some(&attempts) = node_max_attempt.get(&node.id) else {
            continue; // never started — nothing to report.
        };
        let ready_at = if depends_on[&node.id].is_empty() {
            run_start
        } else {
            depends_on[&node.id]
                .iter()
                .filter_map(|dep| last_terminal.get(dep))
                .max()
                .copied()
                .or(run_start)
        };
        let blocked = match (ready_at, first_started.get(&node.id)) {
            (Some(ready), Some(started)) if *started > ready => {
                (*started - ready).to_std().unwrap_or(Duration::ZERO)
            }
            _ => Duration::ZERO,
        };
        nodes.push(NodeStat {
            node_id: node.id.clone(),
            role: node_role.get(&node.id).cloned(),
            tokens: node_tokens.get(&node.id).copied().unwrap_or_default(),
            attempts,
            active: node_active.get(&node.id).copied().unwrap_or(Duration::ZERO),
            blocked,
        });
    }

    let total = state.total_tokens.input + state.total_tokens.output;
    let rework_total = rework_tokens.input + rework_tokens.output;
    let rework_rate = if total == 0 {
        None
    } else {
        Some(rework_total as f64 / total as f64)
    };
    let cache_rate = state
        .total_tokens
        .cached
        .map(|cached| cached as f64 / state.total_tokens.input.max(1) as f64);

    RunStats {
        cptv: cptv(&state),
        rework_rate,
        cache_rate,
        total_tokens: state.total_tokens,
        tasks_total: state.tasks.len(),
        tasks_done: state
            .tasks
            .values()
            .filter(|s| matches!(s, TaskStatus::Done))
            .count(),
        wall_clock,
        nodes,
    }
}

#[allow(clippy::too_many_arguments)]
fn close_attempt(
    node_id: &Option<NodeId>,
    at: DateTime<Utc>,
    tokens: TokenUsage,
    open: &mut HashMap<NodeId, OpenAttempt>,
    node_tokens: &mut HashMap<NodeId, TokenUsage>,
    rework_tokens: &mut TokenUsage,
    node_active: &mut HashMap<NodeId, Duration>,
    last_terminal: &mut HashMap<NodeId, DateTime<Utc>>,
) {
    let Some(node_id) = node_id else { return };
    last_terminal.insert(node_id.clone(), at);
    let entry = node_tokens.entry(node_id.clone()).or_default();
    *entry = sum_tokens(*entry, tokens);
    let Some(opened) = open.remove(node_id) else {
        return;
    };
    let duration = (at - opened.started_at).to_std().unwrap_or(Duration::ZERO);
    *node_active.entry(node_id.clone()).or_default() += duration;
    if opened.attempt > 1 {
        *rework_tokens = sum_tokens(*rework_tokens, tokens);
    }
}

struct OpenAttempt {
    attempt: u32,
    started_at: DateTime<Utc>,
}

fn sum_tokens(a: TokenUsage, b: TokenUsage) -> TokenUsage {
    TokenUsage {
        input: a.input + b.input,
        output: a.output + b.output,
        cached: match (a.cached, b.cached) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        },
    }
}

// --- History across runs of the same workflow (§8.4's `--workflow`, §8.6) --

/// One past run's contribution to a workflow's history — enough to drive
/// `--workflow`'s comparison table and §8.6's prior estimation, without
/// this module doing any storage I/O itself (that's the CLI's job, same
/// functional-core/imperative-shell split as everywhere else).
#[derive(Debug, Clone, PartialEq)]
pub struct RunSummary {
    pub run_id: RunId,
    pub mode: String,
    pub workflow_hash: String,
    pub tokens: u64,
    pub wall_clock: Option<Duration>,
    pub tasks_total: usize,
    pub cptv: Option<f64>,
}

/// Builds one run's summary from its workflow and event log — the per-run
/// unit `--workflow`'s aggregate views fold over.
pub fn run_summary(
    run_id: RunId,
    mode: String,
    workflow_hash: String,
    workflow: &Workflow,
    events: &[Event],
) -> RunSummary {
    let stats = compute_run_stats(workflow, events);
    RunSummary {
        run_id,
        mode,
        workflow_hash,
        tokens: stats.total_tokens.input + stats.total_tokens.output,
        wall_clock: stats.wall_clock,
        tasks_total: stats.tasks_total,
        cptv: stats.cptv,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Percentiles {
    pub median: f64,
    pub p90: f64,
}

/// Nearest-rank percentile over an already-sorted-ascending slice —
/// deterministic (no interpolation to disagree about between two
/// implementations), which is what makes a golden test on this
/// reproducible.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// §8.6's prior estimation: median and p90 of tokens, wall-clock and task
/// count over a workflow's own run history. `None` with fewer than three
/// samples — "sin datos suficientes, el engine no dice nada" is the
/// section's own line, not a threshold this function invented.
#[derive(Debug, Clone, PartialEq)]
pub struct PriorEstimation {
    pub sample_count: usize,
    pub tokens: Percentiles,
    pub wall_clock_secs: Percentiles,
    pub tasks: Percentiles,
}

pub const MIN_SAMPLES_FOR_ESTIMATION: usize = 3;

/// §8.6/DI-05: the informative — never blocking — line `yunta run`
/// prints when the declared run budget sits below what history says this
/// workflow typically needs. `None` without a cap to compare, or without
/// enough history (the estimation's own ≥3-run floor).
pub fn budget_p90_warning(
    cap: Option<u64>,
    estimation: Option<&PriorEstimation>,
) -> Option<String> {
    let cap = cap?;
    let estimation = estimation?;
    if (cap as f64) < estimation.tokens.p90 {
        Some(format!(
            "warning: `limits.max_tokens_per_run` ({cap}) is below this workflow's \
             historical p90 ({:.0} tokens over {} run(s)) — the run may pause on its budget",
            estimation.tokens.p90, estimation.sample_count
        ))
    } else {
        None
    }
}

pub fn prior_estimation(history: &[RunSummary]) -> Option<PriorEstimation> {
    if history.len() < MIN_SAMPLES_FOR_ESTIMATION {
        return None;
    }

    let mut tokens: Vec<f64> = history.iter().map(|r| r.tokens as f64).collect();
    let mut wall_clock: Vec<f64> = history
        .iter()
        .map(|r| r.wall_clock.map(|d| d.as_secs_f64()).unwrap_or(0.0))
        .collect();
    let mut tasks: Vec<f64> = history.iter().map(|r| r.tasks_total as f64).collect();
    for series in [&mut tokens, &mut wall_clock, &mut tasks] {
        series.sort_by(|a, b| a.total_cmp(b));
    }

    Some(PriorEstimation {
        sample_count: history.len(),
        tokens: Percentiles {
            median: percentile(&tokens, 50.0),
            p90: percentile(&tokens, 90.0),
        },
        wall_clock_secs: Percentiles {
            median: percentile(&wall_clock, 50.0),
            p90: percentile(&wall_clock, 90.0),
        },
        tasks: Percentiles {
            median: percentile(&tasks, 50.0),
            p90: percentile(&tasks, 90.0),
        },
    })
}
