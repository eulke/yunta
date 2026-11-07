//! A workflow's own run history: each past run reduced to a summary, and
//! the prior estimation `yunta stats --workflow` compares a new run
//! against — median and p90 of tokens, wall-clock and task count over the
//! runs that actually happened. Nothing here touches storage: the caller
//! hands in each run's workflow and log, as every reading of a run in this
//! crate takes its inputs.
//!
//! With fewer than [`MIN_SAMPLES`] runs the estimation
//! says nothing at all — too little history is reported as too little
//! history, never folded into a number that would read like knowledge.

use std::time::Duration;

use crate::stats::{median, RunStats};
use crate::RunState;
use yunta_core::caution::Caution;
use yunta_core::events::StoredEvent;
use yunta_core::{ContentHash, ModeName, RunId, Workflow};

/// One past run's contribution to a workflow's history — enough to drive
/// `--workflow`'s comparison table and the prior estimation below.
#[derive(Debug, Clone, PartialEq)]
pub struct RunSummary {
    pub run_id: RunId,
    pub mode: ModeName,
    pub workflow_hash: ContentHash,
    pub tokens: u64,
    pub wall_clock: Option<Duration>,
    pub tasks_total: usize,
    pub cptv: Option<f64>,
}

impl RunSummary {
    /// One run's summary, from its workflow and event log — the per-run
    /// unit `--workflow`'s aggregate views fold over. The log is replayed
    /// once, and its mode is the one the state reads off `run_created`.
    pub fn of(
        run_id: RunId,
        workflow_hash: ContentHash,
        workflow: &Workflow,
        events: &[StoredEvent],
    ) -> RunSummary {
        let state = RunState::replay(events);
        let stats = RunStats::as_logged(&state, workflow);
        RunSummary {
            run_id,
            mode: state.run.mode().clone(),
            workflow_hash,
            tokens: state.spend().so_far().total(),
            wall_clock: stats.wall_clock,
            tasks_total: stats.tasks_total,
            cptv: stats.cptv,
        }
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
/// reproducible. `None` for an empty slice: a percentile of nothing has no
/// value.
fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted.get(idx.min(sorted.len() - 1)).copied()
}

/// Median and p90 of an already-sorted-ascending slice, or `None` when it
/// holds no samples to summarize.
fn percentiles(sorted: &[f64]) -> Option<Percentiles> {
    Some(Percentiles {
        median: median(sorted)?,
        p90: percentile(sorted, 90.0)?,
    })
}

/// Prior estimation: median and p90 of tokens, wall-clock and task
/// count over a workflow's own run history. `None` below
/// [`MIN_SAMPLES`] samples: with too little history it says nothing at
/// all, a floor this type reads rather than invents.
#[derive(Debug, Clone, PartialEq)]
pub struct PriorEstimation {
    pub sample_count: usize,
    pub tokens: Percentiles,
    /// `None` when no run in the history reported a measurable wall-clock —
    /// a run without one contributes no fabricated zero-second sample.
    pub wall_clock_secs: Option<Percentiles>,
    pub tasks: Percentiles,
}

/// Fewer independent samples than this say nothing: one outlier decides a
/// percentile of fewer, and a number without a distribution behind it reads
/// like knowledge it is not. Every signal read across past runs, an
/// estimate or a verification finding, keeps this floor.
pub const MIN_SAMPLES: usize = 3;

/// What a declared run budget sitting below what history says this
/// workflow typically needs means for the run about to start —
/// informative, never blocking. `None` without a cap to compare, or
/// without enough history (the estimation's own ≥3-run floor).
///
/// The caution carries the facts and nothing else: how it is worded, and
/// how it is marked on a surface, are that surface's decision.
pub fn budget_caution(cap: Option<u64>, estimation: Option<&PriorEstimation>) -> Option<Caution> {
    let (cap, estimation) = (cap?, estimation?);
    ((cap as f64) < estimation.tokens.p90).then_some(Caution::BudgetBelowHistory {
        cap,
        p90: estimation.tokens.p90,
        samples: estimation.sample_count,
    })
}

impl PriorEstimation {
    /// What a workflow's past runs say a new one costs; `None` below
    /// [`MIN_SAMPLES`] of them.
    pub fn of(history: &[RunSummary]) -> Option<PriorEstimation> {
        if history.len() < MIN_SAMPLES {
            return None;
        }

        let mut tokens: Vec<f64> = history.iter().map(|r| r.tokens as f64).collect();
        // Only the runs that measured a wall-clock: a missing one is left out,
        // never counted as zero seconds.
        let mut wall_clock: Vec<f64> = history
            .iter()
            .filter_map(|r| r.wall_clock.map(|d| d.as_secs_f64()))
            .collect();
        let mut tasks: Vec<f64> = history.iter().map(|r| r.tasks_total as f64).collect();
        for series in [&mut tokens, &mut wall_clock, &mut tasks] {
            series.sort_by(|a, b| a.total_cmp(b));
        }

        // `tokens` and `tasks` always carry one sample per run (≥ 3 here), so
        // their percentiles are always present; `wall_clock` may be empty.
        Some(PriorEstimation {
            sample_count: history.len(),
            tokens: percentiles(&tokens)?,
            wall_clock_secs: percentiles(&wall_clock),
            tasks: percentiles(&tasks)?,
        })
    }
}
