//! See [`crate`]. A workflow's history: every past run of it a project knows
//! about, oldest first, each opened, and what an estimate or a check reads off
//! them.
//!
//! One walk over a project's runs answers every reader. Two readings once made
//! it separately — a summary for the sparkline and the whole log for the
//! verification analysis — and both found a run's directory by joining the
//! current runs root, so a run created under the default state root was
//! invisible to both. Opening a run is where it is found, so the walk goes
//! through it.

use chrono::{DateTime, Utc};
use yunta_core::events::{PauseReason, StoredEvent};
use yunta_core::{Manifest, ModeName, RunId, Workflow, WorkflowName};
use yunta_replay::frame::{RunFrame, RunPhase, WaitingOn};
use yunta_replay::history::{PriorEstimation, RunSummary};
use yunta_replay::verification_effectiveness::VerificationFindings;
use yunta_storage::Storage;

use crate::{OpenedRun, Project};

/// Every past run of a workflow a project knows about, oldest first, each
/// opened.
///
/// Not render's history page, which a person reads drawn from it.
pub struct WorkflowHistory<'p> {
    runs: Vec<OpenedRun<'p>>,
}

impl Project {
    /// Every past run of `workflow` this project knows about, oldest first,
    /// each opened. A run whose manifest cannot be read, or that belongs to
    /// another workflow, is skipped, and a store that cannot be read holds no
    /// history: what is drawn from a history degrades past what it cannot
    /// use.
    pub async fn history(&self, workflow: &WorkflowName) -> WorkflowHistory<'_> {
        let run_ids: Vec<RunId> = Storage::open(&self.storage_path)
            .and_then(|storage| storage.list_runs())
            .map(|runs| runs.into_iter().map(|run| run.run_id).collect())
            .unwrap_or_default();
        let mut dated: Vec<(DateTime<Utc>, OpenedRun<'_>)> = Vec::new();
        for run_id in run_ids {
            let Ok(open) = self.run(&run_id).await else {
                continue;
            };
            let Some(first) = open.events.first() else {
                continue;
            };
            if open.manifest.doc.workflow.name != *workflow {
                continue;
            }
            dated.push((first.timestamp, open));
        }
        dated.sort_by_key(|(timestamp, _)| *timestamp);
        WorkflowHistory {
            runs: dated.into_iter().map(|(_, open)| open).collect(),
        }
    }

    /// What a run of `manifest`'s workflow in `mode` is estimated to cost,
    /// from the runs of it that started in that mode under its hash: every one
    /// for a run being created, which passes no `ahead`; the ones that started
    /// ahead of a run already going, which passes its start, so the estimate
    /// stays the one its creation read whatever ran after it. `None` below the
    /// history an estimate needs.
    pub async fn prior_estimation(
        &self,
        manifest: &Manifest,
        mode: &ModeName,
        ahead: Option<DateTime<Utc>>,
    ) -> Option<PriorEstimation> {
        PriorEstimation::of(&self.budget_sample(manifest, mode, ahead).await)
    }

    /// The runs a budget estimate reads: the summaries of `manifest`'s
    /// workflow's runs that started in `mode` under its hash, and, when
    /// `ahead` is given, the ones that started ahead of it.
    async fn budget_sample(
        &self,
        manifest: &Manifest,
        mode: &ModeName,
        ahead: Option<DateTime<Utc>>,
    ) -> Vec<RunSummary> {
        let mut history = self.history(&manifest.workflow.name).await;
        history
            .runs
            .retain(|open| match (ahead, open.events.first()) {
                (Some(ahead), Some(first)) => first.timestamp < ahead,
                (Some(_), None) => false,
                (None, _) => true,
            });
        history
            .summaries()
            .into_iter()
            .filter(|run| run.mode == *mode && run.workflow_hash == manifest.workflow_hash)
            .collect()
    }

    /// The estimate a run parked on its token budget is read against, so its
    /// caution says what the run was told when it was created: `None` for any
    /// other run, since no other page needs the history.
    pub async fn budget_prior(
        &self,
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
        self.prior_estimation(manifest, &frame.mode, Some(started))
            .await
    }
}

impl WorkflowHistory<'_> {
    /// Whether the workflow has never run here.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// One summary per past run, oldest first: what a sparkline, a mode table
    /// and a prior estimation fold over.
    pub fn summaries(&self) -> Vec<RunSummary> {
        self.runs
            .iter()
            .map(|open| {
                RunSummary::of(
                    open.id.clone(),
                    open.manifest.doc.workflow_hash.clone(),
                    &open.manifest.doc.workflow,
                    &open.events,
                )
            })
            .collect()
    }

    /// What the past runs' whole logs say about `workflow`'s checks.
    pub fn findings(&self, workflow: &Workflow) -> VerificationFindings {
        let logs: Vec<Vec<StoredEvent>> =
            self.runs.iter().map(|open| open.events.clone()).collect();
        VerificationFindings::of(workflow, &logs)
    }

    /// The workflow the latest run froze: the shape the past runs exercised,
    /// which is not necessarily the one on disk now.
    pub fn latest_workflow(&self) -> Option<&Workflow> {
        self.runs.last().map(|open| &open.manifest.doc.workflow)
    }
}
