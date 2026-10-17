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
mod time;
mod workflow;

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_core::events::{PauseReason, StoredEvent};
use yunta_core::{Manifest, ModeName, RunId, Workflow, WorkflowName};
use yunta_engine::{
    compute_run_stats_of, prior_estimation, run_summary, PriorEstimation, RunFrame, RunPhase,
    RunSummary, VerificationFindings, WaitingOn,
};

use crate::context::{Context, Opened};
use crate::error::{CliError, Outcome};
use crate::output::{emit, Answer, Mode};
use crate::render::doc::Doc;
use crate::render::{duration, Look, Tokens};
use json::{RunStatsJson, WorkflowHistoryJson};
use run::RunStatsPage;
pub(crate) use workflow::verification_findings;
use workflow::WorkflowPage;

/// Prints what `run_id` spent, or what the history of `workflow` shows, as
/// the document `output` asks for.
pub async fn stats(
    run_id: Option<&RunId>,
    workflow: Option<&WorkflowName>,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    match (run_id, workflow) {
        (Some(run_id), None) => stats_run(run_id, output, look).await,
        (None, Some(workflow)) => stats_workflow(workflow, output, look).await,
        (None, None) => Err(CliError::msg(
            "`yunta stats` needs a run id or `--workflow <name>`",
        )),
        (Some(_), Some(_)) => Err(CliError::msg(
            "`yunta stats` takes a run id or `--workflow <name>`, not both",
        )),
    }
}

/// What the run's spend costs at its model's price: one blended line when
/// several models are priced, and none when no price is known.
fn currency_line(
    tokens: u64,
    pricing: Option<&BTreeMap<String, yunta_core::PricingEntry>>,
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
        "~{estimate:.2} (avg of {}, never authoritative)",
        yunta_core::text::counted(pricing.len(), "priced model")
    ))
}

/// What `yunta stats <run_id>` answers: the run's document for a program,
/// and its block for a person.
#[derive(serde::Serialize)]
struct RunStatsAnswer<'a> {
    #[serde(flatten)]
    json: RunStatsJson,
    #[serde(skip)]
    page: RunStatsPage<'a>,
}

impl Answer for RunStatsAnswer<'_> {
    fn document<'a>(&'a self, look: &Look) -> Doc<'a> {
        self.page.document(look)
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}

async fn stats_run(run_id: &RunId, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let open = ctx.open_run(run_id).await?;
    let events = open.events;
    let manifest = open.manifest.doc;

    let state = yunta_engine::derive(&events);
    let run_stats = compute_run_stats_of(&manifest.workflow, &state);
    let mode = state.run.mode().as_str();
    let pricing = ctx.project.config.pricing.clone();
    let answer = RunStatsAnswer {
        json: RunStatsJson::from(run_id, mode, &run_stats, pricing.as_ref()),
        page: RunStatsPage {
            run_id,
            mode,
            stats: &run_stats,
            state: &state,
            pricing: pricing.as_ref(),
        },
    };
    emit(output, &answer, look)
}

/// What `yunta stats --workflow <name>` answers: the history as a document
/// for a program, and as the page a person reads.
#[derive(serde::Serialize)]
struct WorkflowAnswer<'a> {
    #[serde(flatten)]
    json: WorkflowHistoryJson,
    #[serde(skip)]
    page: WorkflowPage<'a>,
}

impl Answer for WorkflowAnswer<'_> {
    fn document<'a>(&'a self, look: &Look) -> Doc<'a> {
        self.page.document(look)
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}

async fn stats_workflow(
    workflow_name: &WorkflowName,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;

    let opened = history(&ctx, workflow_name).await;
    let history = summaries(&opened);
    // A person with no history to show is told so, and nothing else is
    // read: the findings are drawn from every past log, and an empty
    // history has none. A program gets the empty document all the same.
    let findings = match output {
        Mode::Human if history.is_empty() => None,
        _ => findings_of(&opened),
    };
    let answer = WorkflowAnswer {
        json: WorkflowHistoryJson::from(workflow_name, &history, findings.as_ref()),
        page: WorkflowPage {
            name: workflow_name,
            history: &history,
            findings,
        },
    };
    emit(output, &answer, look)
}

/// What the past runs' logs say about the workflow's checks, read against
/// the workflow the latest run froze. `None` when there is no such
/// workflow.
fn findings_of(opened: &[Opened]) -> Option<VerificationFindings> {
    // Needs the raw per-run logs `RunSummary` doesn't keep, and the
    // workflow shape those runs actually exercised to match
    // criteria/re-routes/gates against.
    let (raw_history, workflow) = raw_history(opened);
    workflow.map(|wf| yunta_engine::analyze_verification_effectiveness(&wf, &raw_history))
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

/// The runs a budget estimate reads: the summaries of the workflow's runs
/// that started in `mode` under `workflow_hash`, and, when `ahead` is given,
/// the ones that started ahead of it. Creation reads every run it can see, so
/// it passes none; a read of a run already parked passes that run's start, so
/// the sample stays the one its creation read whatever ran after it.
pub(crate) async fn budget_sample(
    ctx: &Context,
    manifest: &Manifest,
    mode: &ModeName,
    ahead: Option<DateTime<Utc>>,
) -> Vec<RunSummary> {
    let opened: Vec<Opened> = history(ctx, &manifest.workflow.name)
        .await
        .into_iter()
        .filter(|open| match (ahead, open.events.first()) {
            (Some(ahead), Some(first)) => first.timestamp < ahead,
            (Some(_), None) => false,
            (None, _) => true,
        })
        .collect();
    summaries(&opened)
        .into_iter()
        .filter(|run| run.mode == *mode && run.workflow_hash == manifest.workflow_hash)
        .collect()
}

/// The estimate a run parked on its token budget is read against, so its
/// caution says what the run was told when it was created. Read only for
/// that pause: no other page needs the history.
pub(crate) async fn budget_prior(
    ctx: &Context,
    manifest: &Manifest,
    frame: &RunFrame,
    events: &[StoredEvent],
) -> Option<PriorEstimation> {
    let parked_on_budget = matches!(
        &frame.phase,
        RunPhase::Waiting {
            on: WaitingOn::Run {
                reason: PauseReason::BudgetExhausted { .. },
            },
        }
    );
    if !parked_on_budget {
        return None;
    }
    let started = events.first()?.timestamp;
    let sample = budget_sample(ctx, manifest, &frame.mode, Some(started)).await;
    prior_estimation(&sample)
}

/// What `--workflow`'s sparkline, mode table and prior estimation fold
/// over: one summary per past run, oldest first.
pub(crate) fn summaries(history: &[Opened]) -> Vec<RunSummary> {
    history
        .iter()
        .map(|open| {
            run_summary(
                open.run_id.clone(),
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
pub(crate) fn format_estimation_line(
    estimation: &yunta_engine::PriorEstimation,
    glyphs: crate::render::Glyphs,
) -> String {
    let sep = glyphs.sep();
    let wall_clock = match estimation.wall_clock_secs {
        Some(p) => duration(Duration::from_secs_f64(p.median)),
        None => "n/a".to_string(),
    };
    format!(
        "{} {sep} median {}, p90 {} {sep} median wall-clock {}",
        yunta_core::text::counted(estimation.sample_count, "past run"),
        Tokens::rounded(estimation.tokens.median),
        Tokens::rounded(estimation.tokens.p90).figure(),
        wall_clock,
    )
}
