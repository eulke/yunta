//! The scope-expansion request an attempt's session left behind, read
//! and decided once the session has ended.

use super::attempt::AttemptParams;
use super::TaskCycleError;
use crate::scope_expansion::{ScopeExpansionOutcome, ScopeExpansionRequest};

/// Reads the agent's own scope-expansion request from this attempt's
/// worktree (a fresh session per attempt leaves it there, not on the log)
/// and decides it: refused when it reaches what is denied, granted on the
/// evidence it cites when that evidence holds, and otherwise evaluated
/// against the node's declared mode, `within` set and cap. `None` when the
/// attempt left no request — the ordinary case.
pub(super) async fn evaluate_scope_expansion(
    params: &AttemptParams<'_>,
) -> Result<Option<ScopeExpansionOutcome>, TaskCycleError> {
    let Some(request) = load(params).await? else {
        return Ok(None);
    };
    let request = match crate::scope_expansion::refuse_what_is_denied(request, params.denied) {
        Ok(refused) => return Ok(Some(refused)),
        Err(request) => request,
    };
    if let Some(evidenced) = super::evidence::of_request(params, &request).await? {
        return Ok(Some(evidenced));
    }
    by_policy(params, request).await.map(Some)
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

/// `request` decided by the loop's mode, its `within` ceiling and its cap.
async fn by_policy(
    params: &AttemptParams<'_>,
    request: ScopeExpansionRequest,
) -> Result<ScopeExpansionOutcome, TaskCycleError> {
    let policy = params.scope_expansion;
    let (precheck_exit, decision) = crate::scope_expansion::evaluate(
        policy.map(|se| se.mode).unwrap_or_default(),
        policy.map(|se| se.within.as_slice()).unwrap_or(&[]),
        policy.and_then(|se| se.max_per_run),
        params.max_expansion_files,
        params.grants,
        &request,
        params.unit.worktree.as_path(),
        params.supervision,
    )
    .await
    .map_err(|source| TaskCycleError::ScopeExpansion {
        task: params.task.id.clone(),
        source,
    })?;
    Ok(ScopeExpansionOutcome {
        request,
        precheck_exit,
        decision,
    })
}
