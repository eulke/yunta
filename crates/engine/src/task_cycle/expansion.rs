//! The scope-expansion request an attempt's session left behind, read
//! and decided once the session has ended.

use yunta_core::expansion::{Ask, Bounds, Decision, Denial, Precheck, Proposal};
use yunta_core::ScopeGlob;

use super::attempt::AttemptParams;
use super::evidence::Located;
use super::TaskCycleError;
use crate::scope_expansion::{ScopeExpansionOutcome, ScopeExpansionRequest};

/// Whether a decision stands without a red output's say: a grant, or a
/// refusal for what no session may write or for the mode. A red output never
/// reopens either.
pub(super) fn standing(decision: &Decision) -> bool {
    matches!(
        decision,
        Decision::Grant { .. } | Decision::Deny(Denial::ReachesProtected { .. } | Denial::ModeDeny)
    )
}

/// The paths an outcome grants: none unless its decision is a grant.
pub(super) fn paths_granted(outcome: Option<&ScopeExpansionOutcome>) -> &[ScopeGlob] {
    match outcome.map(|outcome| &outcome.decision) {
        Some(Decision::Grant { paths, .. }) => paths,
        _ => &[],
    }
}

/// Reads the agent's own scope-expansion request from this attempt's
/// worktree (a fresh session per attempt leaves it there, not on the log)
/// and decides it. A request citing one of the task's criteria that locates
/// its paths is decided as located. Any other request is decided on the
/// pre-check of its proposed criterion. `None` when the attempt left no
/// request — the ordinary case.
pub(super) async fn evaluate_scope_expansion(
    params: &AttemptParams<'_>,
) -> Result<Option<ScopeExpansionOutcome>, TaskCycleError> {
    let Some(request) = load(params).await? else {
        return Ok(None);
    };
    let located = super::evidence::of_request(params, &request).await?;
    if located.is_some() {
        return decide_located(params, located).await;
    }
    let precheck = match &request.proposed_criterion {
        Some(criterion) => Some(run_precheck(params, &criterion.cmd).await?),
        None => None,
    };
    let proposal =
        request
            .proposed_criterion
            .as_ref()
            .zip(precheck)
            .map(|(criterion, precheck)| Proposal {
                cmd: &criterion.cmd,
                precheck,
            });
    let (decision, count) = decide(
        params,
        Ask::Requested {
            paths: &request.paths,
            reason: &request.reason,
            proposal,
        },
    )
    .await?;
    Ok(Some(ScopeExpansionOutcome {
        asked: Some(request),
        precheck,
        decision,
        count,
    }))
}

/// Decides what a red output located: the paths, the criterion that located
/// them, and the request the session made, when it made one. Nothing located
/// decides nothing.
pub(super) async fn decide_located(
    params: &AttemptParams<'_>,
    located: Option<Located>,
) -> Result<Option<ScopeExpansionOutcome>, TaskCycleError> {
    let Some(Located {
        paths,
        criterion,
        asked,
        precheck,
    }) = located
    else {
        return Ok(None);
    };
    let (decision, count) = decide(
        params,
        Ask::Located {
            paths: &paths,
            criterion: &criterion,
        },
    )
    .await?;
    Ok(Some(ScopeExpansionOutcome {
        asked,
        precheck,
        decision,
        count,
    }))
}

/// The request the session left in its checkout, if it left one.
async fn load(params: &AttemptParams<'_>) -> Result<Option<ScopeExpansionRequest>, TaskCycleError> {
    crate::scope_expansion::load_request(params.unit.worktree.as_path())
        .await
        .map_err(|source| TaskCycleError::ScopeExpansion {
            task: params.task.id.clone(),
            source,
        })
}

/// Decides `ask` by the node's mode, file bound, cap and protected set, and
/// counts a grant against the run's window.
async fn decide(
    params: &AttemptParams<'_>,
    ask: Ask<'_>,
) -> Result<(Decision, u32), TaskCycleError> {
    let policy = params.scope_expansion;
    let bounds = Bounds {
        mode: policy.map(|se| se.mode).unwrap_or_default(),
        max_files: params.max_expansion_files,
        max_per_run: policy.and_then(|se| se.max_per_run),
        protected: params.protected,
        beside: params.beside,
    };
    crate::scope_expansion::expand(
        ask,
        &bounds,
        params.grants,
        params.unit.worktree.as_path(),
        params.supervision,
    )
    .await
    .map_err(|source| TaskCycleError::ScopeExpansion {
        task: params.task.id.clone(),
        source,
    })
}

/// The pre-check a proposed criterion gets: its outcome on the attempt's
/// worktree.
async fn run_precheck(params: &AttemptParams<'_>, cmd: &str) -> Result<Precheck, TaskCycleError> {
    crate::scope_expansion::run_criterion(params.unit.worktree.as_path(), cmd, params.supervision)
        .await
        .map_err(|source| TaskCycleError::ScopeExpansion {
            task: params.task.id.clone(),
            source,
        })
}
