//! What a task cycle writes to the log as it verifies: every check the
//! moment it runs, not once the whole cycle is over.
//!
//! A task session reads its task's checks from the log through its own
//! tools, and a retry opens right after the attempt before it: a verdict
//! the log only learned at integration would be one the next attempt
//! could not see. So the cycle records each check here, through its
//! observer, and integration records only what integration decides.

use yunta_core::events::{
    CriteriaCheckedPayload, CriterionResult, CriterionType, EventPayload, NodeEvent, Phase,
    ScopeCheckedPayload,
};
use yunta_core::{NodeId, Seq, TaskId};

use super::{CriterionRun, SessionObserver, TaskCycleError};
use crate::scope::ScopeCheckResult;

/// Where one task's checks are written: the cycle's observer and the
/// node it answers to — or nowhere, for a standalone `run_task`.
#[derive(Clone, Copy)]
pub(super) struct Recorder<'a> {
    pub(super) audit: Option<(&'a dyn SessionObserver, &'a NodeId)>,
    pub(super) task: &'a TaskId,
}

impl Recorder<'_> {
    /// Records what one run of the criteria answered, and returns the
    /// sequence number the log gave it — `None` without an observer.
    pub(super) async fn criteria(
        &self,
        phase: Phase,
        runs: &[CriterionRun],
    ) -> Result<Option<Seq>, TaskCycleError> {
        self.record(EventPayload::Node(NodeEvent::CriteriaChecked(
            CriteriaCheckedPayload {
                task_id: self.task.clone(),
                phase,
                results: to_results(runs),
            },
        )))
        .await
    }

    /// Records what one scope audit found.
    pub(super) async fn scope(&self, scope: &ScopeCheckResult) -> Result<(), TaskCycleError> {
        self.record(EventPayload::Node(NodeEvent::ScopeChecked(
            ScopeCheckedPayload {
                task_id: Some(self.task.clone()),
                diff: scope.diff.clone(),
                violations: scope.violations.clone(),
            },
        )))
        .await
        .map(|_| ())
    }

    async fn record(&self, payload: EventPayload) -> Result<Option<Seq>, TaskCycleError> {
        let Some((observer, node)) = self.audit else {
            return Ok(None);
        };
        observer
            .record(node, payload)
            .await
            .map(Some)
            .map_err(|source| TaskCycleError::Audit {
                task: self.task.clone(),
                source,
            })
    }
}

/// Criterion runs as the log records them.
pub(crate) fn to_results(runs: &[CriterionRun]) -> Vec<CriterionResult> {
    runs.iter()
        .map(|run| CriterionResult {
            cmd: run.cmd.clone(),
            exit_code: run.exit_code,
            r#type: run.is_guard.then_some(CriterionType::Guard),
            reused: run.reused,
            duration_ms: run.duration_ms,
        })
        .collect()
}
