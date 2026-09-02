//! `yunta stats <run_id>` and `yunta stats --workflow <name>`: every
//! number comes from `yunta_engine::stats`'s pure derivation over the
//! event log — this module only gathers the right events off disk
//! (the imperative half) and renders them, either as `--json` or as a
//! terminal visualization (horizontal bars per node/role, a sparkline
//! of a workflow's historical CPTV, a comparison table between modes).
//! Every rendered line stays inside 80 columns and never depends on
//! color — see this module's own render functions for how.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use serde::Serialize;
use yunta_core::events::StoredEvent;
use yunta_core::{Manifest, ModeName, RunId};
use yunta_engine::{
    compute_run_stats, prior_estimation, run_summary, NodeStat, RunStats, RunSummary,
};
use yunta_storage::Storage;

use crate::project::{self, Project};

pub fn stats(run_id: Option<&RunId>, workflow: Option<&str>, json: bool) -> ExitCode {
    match (run_id, workflow) {
        (Some(run_id), None) => stats_run(run_id, json),
        (None, Some(workflow)) => stats_workflow(workflow, json),
        (None, None) => {
            eprintln!("error: `yunta stats` needs a run id or `--workflow <name>`");
            ExitCode::FAILURE
        }
        (Some(_), Some(_)) => {
            eprintln!("error: `yunta stats` takes a run id or `--workflow <name>`, not both");
            ExitCode::FAILURE
        }
    }
}

fn resolve(cwd: &std::path::Path) -> Result<(Project, Storage), ExitCode> {
    let project = project::resolve(cwd).map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })?;
    let storage = Storage::open(&project.storage_path).map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })?;
    Ok((project, storage))
}

fn stats_run(run_id: &RunId, json: bool) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (project, storage) = match resolve(&cwd) {
        Ok(pair) => pair,
        Err(code) => return code,
    };
    let events = match storage.events_for_run(run_id) {
        Ok(events) => events,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if events.is_empty() {
        eprintln!(
            "error: no run `{run_id}` in {}",
            project.storage_path.display()
        );
        return ExitCode::FAILURE;
    }

    // Search order (current runs root, then the default) — the run's
    // own frozen paths take over once the manifest is open.
    let manifest_path = crate::project::find_run_dir(&project, run_id.as_str())
        .unwrap_or_else(|| project.runs_root.join(run_id.as_str()))
        .join("manifest.yaml");
    let manifest: Manifest = match crate::load_yaml(&manifest_path, "run manifest") {
        Ok(manifest) => manifest,
        Err(code) => return code,
    };

    let run_stats = compute_run_stats(&manifest.workflow, &events);
    let mode = yunta_core::events::run_mode(&events);
    let pricing = project.config.pricing.clone();

    if json {
        let dto = RunStatsJson::from(run_id, mode.as_str(), &run_stats, pricing.as_ref());
        println!("{}", serde_json::to_string_pretty(&dto).unwrap());
    } else {
        render_run_stats(run_id, mode.as_str(), &run_stats, pricing.as_ref());
    }
    ExitCode::SUCCESS
}

fn stats_workflow(workflow_name: &str, json: bool) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (project, storage) = match resolve(&cwd) {
        Ok(pair) => pair,
        Err(code) => return code,
    };

    let history = collect_history(&project.runs_root, &storage, workflow_name);
    if history.is_empty() {
        println!("no runs of workflow `{workflow_name}` yet");
        return ExitCode::SUCCESS;
    }

    // Needs the raw per-run logs `RunSummary` doesn't keep, and the
    // workflow shape those runs actually exercised to match
    // criteria/re-routes/gates against.
    let (raw_history, workflow) = collect_raw_history(&project.runs_root, &storage, workflow_name);
    let findings = workflow
        .as_ref()
        .map(|wf| yunta_engine::analyze_verification_effectiveness(wf, &raw_history));

    if json {
        let dto = WorkflowHistoryJson::from(workflow_name, &history, findings.as_ref());
        println!("{}", serde_json::to_string_pretty(&dto).unwrap());
    } else {
        render_workflow_history(workflow_name, &history);
        if let Some(findings) = &findings {
            let text = render_verification_findings(findings);
            if !text.is_empty() {
                println!("\n{text}");
            }
        }
    }
    ExitCode::SUCCESS
}

/// Every past run of `workflow_name` this project's storage knows about,
/// oldest first — the unit `--workflow`'s sparkline, mode table and
/// prior estimation all fold over. Skips a run whose manifest is
/// unreadable or belongs to a different workflow, same "degrade past
/// what a for-display command can't use" stance `list_runs` already
/// takes for its own unreadable entries.
pub(crate) fn collect_history(
    runs_root: &Path,
    storage: &Storage,
    workflow_name: &str,
) -> Vec<RunSummary> {
    let run_ids: Vec<RunId> = storage
        .list_runs()
        .map(|runs| runs.into_iter().map(|run| run.run_id).collect())
        .unwrap_or_default();
    let mut dated: Vec<(chrono::DateTime<chrono::Utc>, RunSummary)> = Vec::new();
    for run_id in run_ids {
        let Ok(events) = storage.events_for_run(&run_id) else {
            continue;
        };
        let Some(first) = events.first() else {
            continue;
        };
        let manifest_path = runs_root.join(run_id.as_str()).join("manifest.yaml");
        let Some(manifest) = std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|c| yunta_core::yaml::parse::<Manifest>(&c).ok())
        else {
            continue;
        };
        if manifest.workflow.name != workflow_name {
            continue;
        }
        let mode = yunta_core::events::run_mode(&events);
        let summary = run_summary(
            run_id,
            mode,
            manifest.workflow_hash.clone(),
            &manifest.workflow,
            &events,
        );
        dated.push((first.timestamp, summary));
    }
    dated.sort_by_key(|(ts, _)| *ts);
    dated.into_iter().map(|(_, s)| s).collect()
}

/// Every past run's own full event log for `workflow_name`, plus the
/// most recent run's own frozen workflow definition —
/// [`collect_history`]'s raw-events twin: `RunSummary` throws away
/// exactly the per-criterion/re-route/gate detail
/// `analyze_verification_effectiveness` needs, so this keeps the whole
/// log instead. The returned workflow isn't necessarily byte-identical
/// to what's on disk right now — it's the shape those runs actually
/// exercised, close enough for `analyze` to match nodes/`on_failure`/
/// gates against. Same skip-what-can't-be-read stance as
/// [`collect_history`].
pub(crate) fn collect_raw_history(
    runs_root: &Path,
    storage: &Storage,
    workflow_name: &str,
) -> (Vec<Vec<StoredEvent>>, Option<yunta_core::Workflow>) {
    let run_ids: Vec<RunId> = storage
        .list_runs()
        .map(|runs| runs.into_iter().map(|run| run.run_id).collect())
        .unwrap_or_default();
    let mut logs = Vec::new();
    let mut latest_workflow = None;
    for run_id in run_ids {
        let Ok(events) = storage.events_for_run(&run_id) else {
            continue;
        };
        let Some(first) = events.first() else {
            continue;
        };
        let manifest_path = runs_root.join(run_id.as_str()).join("manifest.yaml");
        let Some(manifest) = std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|c| yunta_core::yaml::parse::<Manifest>(&c).ok())
        else {
            continue;
        };
        if manifest.workflow.name != workflow_name {
            continue;
        }
        let is_newer = match &latest_workflow {
            Some((ts, _)) => first.timestamp > *ts,
            None => true,
        };
        if is_newer {
            latest_workflow = Some((first.timestamp, manifest.workflow));
        }
        logs.push(events);
    }
    (logs, latest_workflow.map(|(_, wf)| wf))
}

// --- Terminal rendering — colorless by construction: degrading
// without color isn't a fallback mode, it's the only mode. --------------

const BAR_WIDTH: usize = 20;
const LABEL_WIDTH: usize = 12;
const SPARK_CHARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

fn bar(value: u64, max: u64) -> String {
    if max == 0 {
        return "·".repeat(BAR_WIDTH);
    }
    let filled = (((value as f64 / max as f64) * BAR_WIDTH as f64).round() as usize).min(BAR_WIDTH);
    format!("{}{}", "█".repeat(filled), "·".repeat(BAR_WIDTH - filled))
}

fn truncate(s: &str, width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= width {
        format!("{s:<width$}")
    } else {
        let mut t: String = chars[..width.saturating_sub(1)].iter().collect();
        t.push('…');
        format!("{t:<width$}")
    }
}

fn format_duration(d: Duration) -> String {
    let total = d.as_secs();
    if total < 60 {
        format!("{total}s")
    } else if total < 3600 {
        format!("{}m{:02}s", total / 60, total % 60)
    } else {
        format!("{}h{:02}m", total / 3600, (total % 3600) / 60)
    }
}

fn format_pct(fraction: f64) -> String {
    format!("{:>3.0}%", fraction * 100.0)
}

fn currency_line(
    tokens: u64,
    pricing: Option<&std::collections::HashMap<String, yunta_core::PricingEntry>>,
) -> Option<String> {
    // A currency estimate needs a *model* to price against; with more
    // than one model priced and no per-node attribution surfaced here,
    // showing one blended-average line beats showing none — never
    // silently picking the first entry a HashMap happens to iterate.
    let pricing = pricing?;
    if pricing.is_empty() {
        return None;
    }
    let avg_per_1k: f64 = pricing
        .values()
        .map(|entry| entry.cost_per_1k_tokens)
        .sum::<f64>()
        / pricing.len() as f64;
    let estimate = (tokens as f64 / 1000.0) * avg_per_1k;
    Some(format!(
        "  ~{estimate:.2} (avg of {} priced model(s), never authoritative)",
        pricing.len()
    ))
}

fn render_run_stats(
    run_id: &RunId,
    mode: &str,
    stats: &RunStats,
    pricing: Option<&std::collections::HashMap<String, yunta_core::PricingEntry>>,
) {
    println!("run {run_id} — mode {mode}");
    if !stats.unknown_kinds.is_empty() {
        let kinds: Vec<String> = stats
            .unknown_kinds
            .iter()
            .map(|count| format!("{} ×{}", count.kind, count.events))
            .collect();
        println!(
            "unknown event kind(s), interpreted partially: {}",
            kinds.join(", ")
        );
    }
    match stats.cptv {
        Some(cptv) => println!(
            "CPTV: {cptv:.1} tokens/task done ({} done)",
            stats.tasks_done
        ),
        None => println!("CPTV: n/a (no task done yet)"),
    }
    match stats.rework_rate {
        Some(rate) => println!("rework rate: {}", format_pct(rate)),
        None => println!("rework rate: n/a"),
    }
    match stats.cache_rate {
        Some(rate) => println!("cache rate: {}", format_pct(rate)),
        None => println!("cache rate: n/a (adapter never reported it)"),
    }
    let total = stats.total_tokens.input + stats.total_tokens.output;
    print!(
        "tokens: {} in / {} out",
        stats.total_tokens.input, stats.total_tokens.output
    );
    if let Some(cached) = stats.total_tokens.cached {
        print!(" ({cached} cached)");
    }
    println!();
    if let Some(line) = currency_line(total, pricing) {
        println!("{line}");
    }

    if !stats.nodes.is_empty() {
        println!("\nnodes:");
        let max_tokens = stats
            .nodes
            .iter()
            .map(|n| n.tokens.input + n.tokens.output)
            .max()
            .unwrap_or(0);
        for node in &stats.nodes {
            println!("{}", node_line(node, max_tokens));
        }
    }

    let by_runner = stats.tokens_by_runner();
    if !by_runner.is_empty() {
        println!("\nrunners:");
        let max_runner_tokens = by_runner
            .iter()
            .map(|(_, t)| t.input + t.output)
            .max()
            .unwrap_or(0);
        for (runner, tokens) in &by_runner {
            let total = tokens.input + tokens.output;
            println!(
                "  {} {}  {total:>8} tok",
                truncate(runner.as_str(), LABEL_WIDTH),
                bar(total, max_runner_tokens),
            );
        }
    }
}

fn node_line(node: &NodeStat, max_tokens: u64) -> String {
    let total = node.tokens.input + node.tokens.output;
    let blocked = node
        .blocked_fraction()
        .map(format_pct)
        .unwrap_or_else(|| " n/a".to_string());
    format!(
        "  {} {}  {total:>8} tok  {:>8}  blk:{blocked}",
        truncate(node.node_id.as_str(), LABEL_WIDTH),
        bar(total, max_tokens),
        format_duration(node.wall_clock()),
    )
}

fn render_workflow_history(workflow_name: &str, history: &[RunSummary]) {
    println!("workflow `{workflow_name}` — {} run(s)", history.len());

    println!("\nCPTV over time:");
    let cptv_series: Vec<f64> = history.iter().map(|r| r.cptv.unwrap_or(0.0)).collect();
    println!(
        "  {}  (oldest -> newest, latest = {})",
        sparkline(&cptv_series),
        history
            .last()
            .and_then(|r| r.cptv)
            .map(|c| format!("{c:.1}"))
            .unwrap_or_else(|| "n/a".to_string())
    );

    println!("\nmodes:");
    for (mode, runs, median_cptv, median_tokens) in mode_table(history) {
        println!(
            "  {} {:>3} run(s)   median CPTV {}   median tokens {}",
            truncate(mode.as_str(), LABEL_WIDTH),
            runs,
            median_cptv
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "n/a".to_string()),
            median_tokens
                .map(|v| format!("{v:.0}"))
                .unwrap_or_else(|| "n/a".to_string()),
        );
    }

    if let Some(estimation) = prior_estimation(history) {
        println!("\n{}", format_estimation_line(&estimation));
    } else {
        println!(
            "\nestimation: not enough runs yet (need {}, have {})",
            yunta_engine::MIN_SAMPLES_FOR_ESTIMATION,
            history.len()
        );
    }
}

/// Verification-effectiveness findings — advisory only, never a reason
/// `stats` or `check` exits non-zero: these are suggestions for a
/// person to weigh,
/// not errors. Returns the rendered text (empty if there's nothing to
/// say) so each caller can send it to stdout (`stats`) or stderr
/// (`check`, alongside its own warnings) without duplicating the
/// wording.
pub(crate) fn render_verification_findings(
    findings: &yunta_engine::VerificationFindings,
) -> String {
    if findings.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("verification performance — advisory, nothing here is acted on automatically:\n");
    for c in &findings.never_red_criteria {
        out.push_str(&format!(
            "  criterion `{}` was never red in pre-check across {} run(s) — \
             either redundant, or mis-written (both readings shown, never just one)\n",
            c.cmd, c.sample_count
        ));
    }
    for r in &findings.never_triggered_reroutes {
        out.push_str(&format!(
            "  node `{}`'s re-route to `{}` never fired across {} run(s) — \
             the prior flow is more reliable than expected\n",
            r.node, r.goto, r.sample_count
        ));
    }
    for g in &findings.always_approved_gates {
        out.push_str(&format!(
            "  gate `{}` was approved without adjustment across {} resolution(s) — \
             still adding value, or become ritual?\n",
            g.node, g.sample_count
        ));
    }
    if let Some(t) = &findings.always_first_try_tasks {
        out.push_str(&format!(
            "  every task passed on its first try across {} task instance(s) — \
             the plan may be cutting too fine\n",
            t.sample_count
        ));
    }
    for m in &findings.unused_modes {
        out.push_str(&format!(
            "  mode `{}` was never chosen across {} run(s) — \
             still worth declaring?\n",
            m.name, m.runs_observed
        ));
    }
    out
}

fn sparkline(values: &[f64]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let max = values.iter().cloned().fold(0.0_f64, f64::max);
    if max <= 0.0 {
        return "·".repeat(values.len());
    }
    values
        .iter()
        .map(|&v| {
            let idx = ((v / max) * (SPARK_CHARS.len() - 1) as f64).round() as usize;
            SPARK_CHARS[idx.min(SPARK_CHARS.len() - 1)]
        })
        .collect()
}

/// Median CPTV/tokens per mode — a plain historical comparison, not a
/// prediction, so it doesn't gate on
/// [`yunta_engine::MIN_SAMPLES_FOR_ESTIMATION`] the way
/// [`prior_estimation`] does: a mode with one run still gets a row, just
/// with that run's own numbers as its "median".
fn mode_table(history: &[RunSummary]) -> Vec<(ModeName, usize, Option<f64>, Option<f64>)> {
    let mut modes: Vec<ModeName> = history.iter().map(|r| r.mode.clone()).collect();
    modes.sort();
    modes.dedup();

    modes
        .into_iter()
        .map(|mode| {
            let runs: Vec<&RunSummary> = history.iter().filter(|r| r.mode == mode).collect();
            let mut cptv: Vec<f64> = runs.iter().filter_map(|r| r.cptv).collect();
            let mut tokens: Vec<f64> = runs.iter().map(|r| r.tokens as f64).collect();
            cptv.sort_by(|a, b| a.total_cmp(b));
            tokens.sort_by(|a, b| a.total_cmp(b));
            (mode, runs.len(), median(&cptv), median(&tokens))
        })
        .collect()
}

fn median(sorted: &[f64]) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    Some(sorted[sorted.len() / 2])
}

/// Shared by `yunta stats --workflow` and `yunta run`/`list_workflows`
/// so the three surfaces never phrase the same numbers differently.
pub(crate) fn format_estimation_line(estimation: &yunta_engine::PriorEstimation) -> String {
    format!(
        "{} past run(s) · median {:.0} tokens, p90 {:.0} · median wall-clock {}",
        estimation.sample_count,
        estimation.tokens.median,
        estimation.tokens.p90,
        format_duration(Duration::from_secs_f64(estimation.wall_clock_secs.median)),
    )
}

// --- `--json` ------------------------------------------------------------

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
}

impl From<&NodeStat> for NodeStatJson {
    fn from(n: &NodeStat) -> Self {
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
        }
    }
}

#[derive(Serialize)]
struct RunStatsJson {
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
    currency_estimate: Option<String>,
    nodes: Vec<NodeStatJson>,
    unknown_kinds: Vec<yunta_engine::UnknownKindCount>,
}

impl RunStatsJson {
    fn from(
        run_id: &RunId,
        mode: &str,
        stats: &RunStats,
        pricing: Option<&std::collections::HashMap<String, yunta_core::PricingEntry>>,
    ) -> Self {
        let total = stats.total_tokens.input + stats.total_tokens.output;
        Self {
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
            currency_estimate: currency_line(total, pricing),
            nodes: stats.nodes.iter().map(NodeStatJson::from).collect(),
            unknown_kinds: stats.unknown_kinds.clone(),
        }
    }
}

#[derive(Serialize)]
struct RunSummaryJson {
    run_id: String,
    mode: String,
    workflow_hash: String,
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
    wall_clock_secs_median: f64,
    wall_clock_secs_p90: f64,
    tasks_median: f64,
    tasks_p90: f64,
}

impl From<&yunta_engine::PriorEstimation> for EstimationJson {
    fn from(e: &yunta_engine::PriorEstimation) -> Self {
        Self {
            sample_count: e.sample_count,
            tokens_median: e.tokens.median,
            tokens_p90: e.tokens.p90,
            wall_clock_secs_median: e.wall_clock_secs.median,
            wall_clock_secs_p90: e.wall_clock_secs.p90,
            tasks_median: e.tasks.median,
            tasks_p90: e.tasks.p90,
        }
    }
}

#[derive(Serialize)]
struct WorkflowHistoryJson {
    workflow: String,
    runs: Vec<RunSummaryJson>,
    estimation: Option<EstimationJson>,
    verification_findings: Option<VerificationFindingsJson>,
}

impl WorkflowHistoryJson {
    fn from(
        workflow: &str,
        history: &[RunSummary],
        findings: Option<&yunta_engine::VerificationFindings>,
    ) -> Self {
        Self {
            workflow: workflow.to_string(),
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
    fn from(findings: &yunta_engine::VerificationFindings) -> Self {
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
