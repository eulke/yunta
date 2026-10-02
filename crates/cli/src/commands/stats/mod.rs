//! `yunta stats <run_id>` and `yunta stats --workflow <name>`: every
//! number comes from `yunta_engine::stats`'s pure derivation over the
//! event log — this module only gathers the right events off disk
//! (the imperative half) and renders them, either as `--json` or as a
//! terminal visualization (horizontal bars per node/runner, a sparkline
//! of a workflow's historical CPTV, a comparison table between modes).
//! Every rendered line stays inside the width stdout gets and reads
//! with its glyphs and color stripped, because the columns, bars,
//! words and glyphs all come from `crate::render`, which is where both
//! rules live.

mod json;
mod run;
mod workflow;

use std::time::Duration;

use yunta_core::events::StoredEvent;
use yunta_core::{RunId, Workflow, WorkflowName};
use yunta_engine::{compute_run_stats, run_summary, RunSummary};

use crate::context::{Context, Opened};
use crate::error::{CliError, Outcome};
use crate::render::{duration, Look, Tokens};
use json::{RunStatsJson, WorkflowHistoryJson};
use run::render_run_stats;
pub(crate) use workflow::render_verification_findings;
use workflow::render_workflow_history;

pub async fn stats(
    run_id: Option<&RunId>,
    workflow: Option<&WorkflowName>,
    json: bool,
) -> Result<Outcome, CliError> {
    match (run_id, workflow) {
        (Some(run_id), None) => stats_run(run_id, json).await,
        (None, Some(workflow)) => stats_workflow(workflow, json).await,
        (None, None) => Err(CliError::msg(
            "`yunta stats` needs a run id or `--workflow <name>`",
        )),
        (Some(_), Some(_)) => Err(CliError::msg(
            "`yunta stats` takes a run id or `--workflow <name>`, not both",
        )),
    }
}

async fn stats_run(run_id: &RunId, json: bool) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let open = ctx.open_run(run_id).await?;
    let events = open.events;
    let manifest = open.manifest.doc;

    let run_stats = compute_run_stats(&manifest.workflow, &events);
    let mode = yunta_core::events::run_mode(&events);
    let pricing = ctx.project.config.pricing.clone();

    if json {
        let dto = RunStatsJson::from(run_id, mode.as_str(), &run_stats, pricing.as_ref());
        return crate::json::print_json(&dto);
    }
    print!(
        "{}",
        render_run_stats(
            run_id,
            mode.as_str(),
            &run_stats,
            &yunta_engine::derive(&events),
            pricing.as_ref(),
            &Look::stdout(),
        )
    );
    Ok(Outcome::Success)
}

async fn stats_workflow(workflow_name: &WorkflowName, json: bool) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;

    let opened = history(&ctx, workflow_name).await;
    let history = summaries(&opened);
    if history.is_empty() {
        println!("no runs of workflow `{workflow_name}` yet");
        return Ok(Outcome::Success);
    }

    // Needs the raw per-run logs `RunSummary` doesn't keep, and the
    // workflow shape those runs actually exercised to match
    // criteria/re-routes/gates against.
    let (raw_history, workflow) = raw_history(&opened);
    let findings = workflow
        .as_ref()
        .map(|wf| yunta_engine::analyze_verification_effectiveness(wf, &raw_history));

    if json {
        let dto = WorkflowHistoryJson::from(workflow_name, &history, findings.as_ref());
        return crate::json::print_json(&dto);
    }
    print!(
        "{}",
        render_workflow_history(workflow_name, &history, Look::stdout())
    );
    if let Some(findings) = &findings {
        let text = render_verification_findings(findings);
        if !text.is_empty() {
            println!("\n{text}");
        }
    }
    Ok(Outcome::Success)
}

/// Every past run of `workflow_name` this project knows about, oldest
/// first, each opened.
///
/// The one walk over a project's history. Two readings used to make it
/// separately — a summary for the sparkline and the whole log for the
/// verification analysis — and both found a run's directory by joining
/// this project's current runs root, so a run created under the default
/// state root was invisible to both. `open_run` is where a run is
/// found, so this walks through it and the two readings fold over what
/// it hands back.
///
/// A run whose manifest cannot be read, or that belongs to a different
/// workflow, is skipped: the same "degrade past what a for-display
/// command cannot use" stance `list_runs` already takes.
pub(crate) async fn history(ctx: &Context, workflow_name: &WorkflowName) -> Vec<Opened> {
    let Ok(storage) = ctx.storage() else {
        return Vec::new();
    };
    let run_ids: Vec<RunId> = storage
        .list_runs()
        .map(|runs| runs.into_iter().map(|run| run.run_id).collect())
        .unwrap_or_default();
    let mut dated: Vec<(chrono::DateTime<chrono::Utc>, Opened)> = Vec::new();
    for run_id in run_ids {
        let Ok(open) = ctx.open_run(&run_id).await else {
            continue;
        };
        let Some(first) = open.events.first() else {
            continue;
        };
        if open.manifest.doc.workflow.name != *workflow_name {
            continue;
        }
        dated.push((first.timestamp, open));
    }
    dated.sort_by_key(|(timestamp, _)| *timestamp);
    dated.into_iter().map(|(_, open)| open).collect()
}

/// What `--workflow`'s sparkline, mode table and prior estimation fold
/// over: one summary per past run, oldest first.
pub(crate) fn summaries(history: &[Opened]) -> Vec<RunSummary> {
    history
        .iter()
        .map(|open| {
            run_summary(
                open.run_id.clone(),
                yunta_core::events::run_mode(&open.events),
                open.manifest.doc.workflow_hash.clone(),
                &open.manifest.doc.workflow,
                &open.events,
            )
        })
        .collect()
}

/// The whole log of every past run, plus the most recent one's own
/// frozen workflow — what `analyze_verification_effectiveness` needs
/// and a `RunSummary` throws away. The workflow is not necessarily
/// byte-identical to what is on disk now: it is the shape those runs
/// actually exercised, which is what the analysis matches against.
pub(crate) fn raw_history(history: &[Opened]) -> (Vec<Vec<StoredEvent>>, Option<Workflow>) {
    let latest = history
        .last()
        .map(|open| open.manifest.doc.workflow.clone());
    (
        history.iter().map(|open| open.events.clone()).collect(),
        latest,
    )
}

/// Shared by `yunta stats --workflow` and `yunta run`/`list_workflows`
/// so the three surfaces never phrase the same numbers differently.
pub(crate) fn format_estimation_line(estimation: &yunta_engine::PriorEstimation) -> String {
    let wall_clock = match estimation.wall_clock_secs {
        Some(p) => duration(Duration::from_secs_f64(p.median)),
        None => "n/a".to_string(),
    };
    format!(
        "{} · median {}, p90 {} · median wall-clock {}",
        yunta_core::text::counted(estimation.sample_count, "past run"),
        Tokens::rounded(estimation.tokens.median),
        Tokens::rounded(estimation.tokens.p90).figure(),
        wall_clock,
    )
}
