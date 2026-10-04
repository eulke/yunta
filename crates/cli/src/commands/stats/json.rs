//! The versioned documents `yunta stats --json` prints for a run and for
//! a workflow's history.

use serde::Serialize;
use yunta_core::{ContentHash, RunId, WorkflowName};
use yunta_engine::{prior_estimation, NodeStat, RunStats, RunSummary};

use super::run::currency_line;

#[derive(Serialize)]
struct NodeStatJson {
    node_id: String,
    runner: Option<String>,
    tokens_input: u64,
    tokens_output: u64,
    tokens_cached: Option<u64>,
    attempts: u32,
    active_secs: f64,
    blocked_secs: f64,
    blocked_fraction: Option<f64>,
    /// What this node handed over, by what the engine answered. A node
    /// that submitted nothing carries zeroes rather than nothing: it
    /// was asked and did not deliver, which is a number, not an absence.
    submissions: yunta_engine::Submissions,
    /// What this node found, by what the engine answered — zeroes for a
    /// node the log carries no finding call from, for the same reason.
    findings: yunta_engine::FindingActivity,
}

impl NodeStatJson {
    /// One node's row, with the per-node counts the run's own maps hold
    /// beside the numbers the node stat carries itself.
    fn of(n: &NodeStat, stats: &RunStats) -> Self {
        Self {
            node_id: n.node_id.to_string(),
            runner: n.runner.as_ref().map(ToString::to_string),
            tokens_input: n.tokens.input,
            tokens_output: n.tokens.output,
            tokens_cached: n.tokens.cached,
            attempts: n.attempts,
            active_secs: n.active.as_secs_f64(),
            blocked_secs: n.blocked.as_secs_f64(),
            blocked_fraction: n.blocked_fraction(),
            submissions: stats
                .submissions_by_node
                .get(&n.node_id)
                .copied()
                .unwrap_or_default(),
            findings: stats
                .findings_by_node
                .get(&n.node_id)
                .copied()
                .unwrap_or_default(),
        }
    }
}

#[derive(Serialize)]
pub(super) struct RunStatsJson {
    schema_version: u32,
    run_id: String,
    mode: String,
    cptv: Option<f64>,
    rework_rate: Option<f64>,
    cache_rate: Option<f64>,
    tokens_input: u64,
    tokens_output: u64,
    tokens_cached: Option<u64>,
    tasks_total: usize,
    tasks_done: usize,
    wall_clock_secs: Option<f64>,
    /// How long the host was suspended inside the run's window — what
    /// `wall_clock_secs` and every node's durations leave out.
    asleep_secs: f64,
    /// That wall-clock, by what it went to.
    time: TimeJson,
    currency_estimate: Option<String>,
    nodes: Vec<NodeStatJson>,
    unknown_kinds: Vec<yunta_engine::UnknownKindCount>,
    /// Every document the run handed over, by what the engine answered.
    submissions: yunta_engine::Submissions,
    /// Every finding call the log carries, by what the engine answered.
    findings: yunta_engine::FindingActivity,
    /// How many findings stand now — the fold over the whole log, where
    /// an update replaces and a withdrawal removes. Never the count of
    /// posts: a reader given only `findings.posted` reads a withdrawn
    /// finding as one that still stands.
    findings_standing: u64,
    /// How many of those a proof or a person settled.
    findings_settled: u64,
}

/// A run's wall-clock by share, in seconds.
#[derive(Serialize)]
struct TimeJson {
    working_secs: f64,
    checks_secs: f64,
    decided_checks_secs: f64,
    measuring_secs: f64,
    people_secs: f64,
    offline_secs: f64,
    parked_secs: f64,
    between_secs: f64,
}

impl TimeJson {
    fn of(time: &yunta_engine::TimeSpent) -> Self {
        TimeJson {
            working_secs: time.working.as_secs_f64(),
            checks_secs: time.checks.as_secs_f64(),
            decided_checks_secs: time.decided_checks.as_secs_f64(),
            measuring_secs: time.measuring.as_secs_f64(),
            people_secs: time.people.as_secs_f64(),
            offline_secs: time.offline.as_secs_f64(),
            parked_secs: time.parked.as_secs_f64(),
            between_secs: time.between.as_secs_f64(),
        }
    }
}

impl RunStatsJson {
    pub(super) fn from(
        run_id: &RunId,
        mode: &str,
        stats: &RunStats,
        pricing: Option<&std::collections::BTreeMap<String, yunta_core::PricingEntry>>,
    ) -> Self {
        let total = stats.total_tokens.total();
        Self {
            schema_version: crate::json::SCHEMA_VERSION,
            run_id: run_id.to_string(),
            mode: mode.to_string(),
            cptv: stats.cptv,
            rework_rate: stats.rework_rate,
            cache_rate: stats.cache_rate,
            tokens_input: stats.total_tokens.input,
            tokens_output: stats.total_tokens.output,
            tokens_cached: stats.total_tokens.cached,
            tasks_total: stats.tasks_total,
            tasks_done: stats.tasks_done,
            wall_clock_secs: stats.wall_clock.map(|d| d.as_secs_f64()),
            asleep_secs: stats.asleep.as_secs_f64(),
            time: TimeJson::of(&stats.time),
            currency_estimate: currency_line(total, pricing),
            nodes: stats
                .nodes
                .iter()
                .map(|node| NodeStatJson::of(node, stats))
                .collect(),
            unknown_kinds: stats.unknown_kinds.clone(),
            submissions: stats.artifact_submissions,
            findings: stats.findings,
            findings_standing: stats.findings_effective,
            findings_settled: stats.findings_settled,
        }
    }
}

#[derive(Serialize)]
struct RunSummaryJson {
    run_id: String,
    mode: String,
    workflow_hash: ContentHash,
    tokens: u64,
    wall_clock_secs: Option<f64>,
    tasks_total: usize,
    cptv: Option<f64>,
}

impl From<&RunSummary> for RunSummaryJson {
    fn from(r: &RunSummary) -> Self {
        Self {
            run_id: r.run_id.to_string(),
            mode: r.mode.to_string(),
            workflow_hash: r.workflow_hash.clone(),
            tokens: r.tokens,
            wall_clock_secs: r.wall_clock.map(|d| d.as_secs_f64()),
            tasks_total: r.tasks_total,
            cptv: r.cptv,
        }
    }
}

#[derive(Serialize)]
struct EstimationJson {
    sample_count: usize,
    tokens_median: f64,
    tokens_p90: f64,
    /// `null` when no run in the history reported a measurable wall-clock.
    wall_clock_secs_median: Option<f64>,
    wall_clock_secs_p90: Option<f64>,
    tasks_median: f64,
    tasks_p90: f64,
}

impl From<&yunta_engine::PriorEstimation> for EstimationJson {
    fn from(e: &yunta_engine::PriorEstimation) -> Self {
        Self {
            sample_count: e.sample_count,
            tokens_median: e.tokens.median,
            tokens_p90: e.tokens.p90,
            wall_clock_secs_median: e.wall_clock_secs.map(|p| p.median),
            wall_clock_secs_p90: e.wall_clock_secs.map(|p| p.p90),
            tasks_median: e.tasks.median,
            tasks_p90: e.tasks.p90,
        }
    }
}

#[derive(Serialize)]
pub(super) struct WorkflowHistoryJson {
    schema_version: u32,
    workflow: WorkflowName,
    runs: Vec<RunSummaryJson>,
    estimation: Option<EstimationJson>,
    verification_findings: Option<VerificationFindingsJson>,
}

impl WorkflowHistoryJson {
    pub(super) fn from(
        workflow: &WorkflowName,
        history: &[RunSummary],
        findings: Option<&yunta_engine::VerificationFindings>,
    ) -> Self {
        Self {
            schema_version: crate::json::SCHEMA_VERSION,
            workflow: workflow.clone(),
            runs: history.iter().map(RunSummaryJson::from).collect(),
            estimation: prior_estimation(history).as_ref().map(EstimationJson::from),
            verification_findings: findings.map(VerificationFindingsJson::from),
        }
    }
}

#[derive(Serialize)]
struct VerificationFindingsJson {
    never_red_criteria: Vec<NeverRedCriterionJson>,
    never_triggered_reroutes: Vec<NeverTriggeredRerouteJson>,
    always_approved_gates: Vec<AlwaysApprovedGateJson>,
    always_first_try_tasks: Option<usize>,
    unused_modes: Vec<UnusedModeJson>,
}

#[derive(Serialize)]
struct UnusedModeJson {
    name: String,
    runs_observed: usize,
}

#[derive(Serialize)]
struct NeverRedCriterionJson {
    cmd: String,
    sample_count: usize,
}

#[derive(Serialize)]
struct NeverTriggeredRerouteJson {
    node: String,
    goto: String,
    sample_count: usize,
}

#[derive(Serialize)]
struct AlwaysApprovedGateJson {
    node: String,
    sample_count: usize,
}

impl VerificationFindingsJson {
    pub(super) fn from(findings: &yunta_engine::VerificationFindings) -> Self {
        Self {
            never_red_criteria: findings
                .never_red_criteria
                .iter()
                .map(|c| NeverRedCriterionJson {
                    cmd: c.cmd.clone(),
                    sample_count: c.sample_count,
                })
                .collect(),
            never_triggered_reroutes: findings
                .never_triggered_reroutes
                .iter()
                .map(|r| NeverTriggeredRerouteJson {
                    node: r.node.to_string(),
                    goto: r.goto.to_string(),
                    sample_count: r.sample_count,
                })
                .collect(),
            always_approved_gates: findings
                .always_approved_gates
                .iter()
                .map(|g| AlwaysApprovedGateJson {
                    node: g.node.to_string(),
                    sample_count: g.sample_count,
                })
                .collect(),
            always_first_try_tasks: findings
                .always_first_try_tasks
                .as_ref()
                .map(|t| t.sample_count),
            unused_modes: findings
                .unused_modes
                .iter()
                .map(|m| UnusedModeJson {
                    name: m.name.to_string(),
                    runs_observed: m.runs_observed,
                })
                .collect(),
        }
    }
}
