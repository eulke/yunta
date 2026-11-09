//! `yunta stats <run_id>` and `yunta stats --workflow <name>`: every
//! number comes from `yunta_engine::stats`'s pure derivation over the
//! event log — this module only gathers the right events off disk
//! (the imperative half) and renders them, either as `--json` or as a
//! terminal visualization (horizontal bars per node/runner, a sparkline
//! of a workflow's historical CPTV, a comparison table between modes).
//! Every rendered line stays inside the width stdout gets and reads
//! with its glyphs and color stripped, because the columns, bars,
//! words and glyphs come from `crate::output` and `yunta_render`, and the
//! width and the glyphs are settled once in `crate::output`, where both
//! rules live.

use yunta_render::page::{History, Page, Stats};

use chrono::{DateTime, Utc};
use yunta_core::events::{PauseReason, StoredEvent};
use yunta_core::{Clock, Manifest, ModeName, RunId, Workflow, WorkflowName};
use yunta_replay::frame::{RunFrame, RunPhase, WaitingOn};
use yunta_replay::history::{PriorEstimation, RunSummary};
use yunta_replay::stats::RunStats;
use yunta_replay::verification_effectiveness::VerificationFindings;

use crate::context::{Context, Opened};
use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};
use surco::doc::Depth;
use surco::look::Look;
use yunta_render::document::stats::{RunStatsJson, WorkflowHistoryJson};

/// What `yunta stats <run_id>` answers: the run's document for a program,
/// and its block for a person.
#[derive(serde::Serialize)]
struct RunStatsAnswer<'a> {
    #[serde(flatten)]
    json: RunStatsJson,
    #[serde(skip)]
    page: Stats<'a>,
}

impl Answer for RunStatsAnswer<'_> {
    fn page(&self) -> impl Page + '_ {
        &self.page
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}

/// Prints what the run `run_id` spent, at `depth`, as the document `output` asks for.
pub(crate) async fn of_run(
    run_id: &RunId,
    depth: Depth,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let open = ctx.open_run(run_id).await?;
    let events = open.events;
    let manifest = open.manifest.doc;

    let state = yunta_replay::RunState::replay(&events);
    // The page reads the run at one instant, its headline's: a run still
    // going counts the wait or the work open now.
    let now = ctx.clock.now();
    let run_stats = RunStats::at(&state, &manifest.workflow, now);
    let mode = state.run.mode().as_str();
    let pricing = ctx.project.config.pricing.clone();
    let frame = yunta_replay::frame::RunFrame::of(
        run_id,
        &manifest.workflow,
        &state,
        None,
        &yunta_engine::read_engine(&open.run_dir, &yunta_engine::lock::SystemProbe),
        now,
    );
    let answer = RunStatsAnswer {
        json: RunStatsJson::from(run_id, mode, &run_stats, pricing.as_ref()),
        page: Stats {
            frame: &frame,
            stats: &run_stats,
            pricing: pricing.as_ref(),
            depth,
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
    page: History<'a>,
}

impl Answer for WorkflowAnswer<'_> {
    fn page(&self) -> impl Page + '_ {
        &self.page
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}

/// Prints what the history of a workflow shows, as the document `output` asks for.
pub(crate) async fn of_workflow(
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
        page: History {
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
    // criteria/reroutes/gates against.
    let (raw_history, workflow) = raw_history(opened);
    workflow.map(|wf| {
        yunta_replay::verification_effectiveness::VerificationFindings::of(&wf, &raw_history)
    })
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
    PriorEstimation::of(&sample)
}

/// What `--workflow`'s sparkline, mode table and prior estimation fold
/// over: one summary per past run, oldest first.
pub(crate) fn summaries(history: &[Opened]) -> Vec<RunSummary> {
    history
        .iter()
        .map(|open| {
            RunSummary::of(
                open.run_id.clone(),
                open.manifest.doc.workflow_hash.clone(),
                &open.manifest.doc.workflow,
                &open.events,
            )
        })
        .collect()
}

/// The whole log of every past run, plus the most recent one's own
/// frozen workflow — what `VerificationFindings::of` needs
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
