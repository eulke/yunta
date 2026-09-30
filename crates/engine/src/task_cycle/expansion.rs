//! The scope-expansion request an attempt's session left behind, read
//! and decided once the session has ended.

use super::attempt::AttemptParams;
use super::TaskCycleError;

/// Reads the agent's own scope-expansion request from this attempt's
/// worktree (a fresh session per attempt leaves it there, not on the log)
/// and evaluates it against the node's declared mode, `within` set and cap.
/// `None` when the attempt left no request — the ordinary case.
pub(super) async fn evaluate_scope_expansion(
    params: &AttemptParams<'_>,
) -> Result<Option<crate::scope_expansion::ScopeExpansionOutcome>, TaskCycleError> {
    let &AttemptParams {
        task,
        unit,
        scope_expansion,
        max_expansion_files,
        grants,
        supervision,
        ..
    } = params;
    let cwd = unit.worktree.as_path();
    let Some(expansion_request) =
        crate::scope_expansion::load_request(cwd)
            .await
            .map_err(|source| TaskCycleError::ScopeExpansion {
                task: task.id.clone(),
                source,
            })?
    else {
        return Ok(None);
    };
    let expansion_request =
        match crate::scope_expansion::refuse_what_is_denied(expansion_request, params.denied) {
            Ok(refused) => return Ok(Some(refused)),
            Err(request) => request,
        };
    let mode = scope_expansion.map(|se| se.mode).unwrap_or_default();
    let within = scope_expansion
        .map(|se| se.within.as_slice())
        .unwrap_or(&[]);
    let max_per_run = scope_expansion.and_then(|se| se.max_per_run);
    let (precheck_exit, decision) = crate::scope_expansion::evaluate(
        mode,
        within,
        max_per_run,
        max_expansion_files,
        grants,
        &expansion_request,
        cwd,
        supervision,
    )
    .await
    .map_err(|source| TaskCycleError::ScopeExpansion {
        task: task.id.clone(),
        source,
    })?;
    Ok(Some(crate::scope_expansion::ScopeExpansionOutcome {
        request: expansion_request,
        precheck_exit,
        decision,
    }))
}
