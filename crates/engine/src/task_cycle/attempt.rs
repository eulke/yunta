//! One attempt of a task: the session it opens, the scope check on what
//! it changed, and what it tells `run_task` to do next.

use std::path::{Path, PathBuf};
use yunta_core::ScopeGlob;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{SessionDeath, TokenUsage};
use yunta_core::port::{Adapter, Budget, PermissionProfile};
use yunta_core::Task;

use super::criteria::{post_check, Memo};
use super::session::{dispatch_session, DispatchError, SessionObserver, SessionSetup};
use super::{AttemptRecord, DispatchOutcome, TaskCycleError, TaskOutcome};
use crate::process::Supervision;
use crate::scope::audit;

/// Everything one attempt of [`run_task`] reads: the per-cycle context that
/// never changes between attempts, so an attempt takes just this and its
/// number.
pub(super) struct AttemptParams<'a> {
    pub(super) task: &'a Task,
    pub(super) instruction: &'a str,
    pub(super) adapter: &'a dyn Adapter,
    pub(super) node: &'a yunta_core::Node,
    pub(super) cwd: &'a Path,
    pub(super) budget: Budget,
    pub(super) memo: &'a Memo,
    pub(super) profile: PermissionProfile,
    pub(super) scope_expansion: Option<&'a yunta_core::ScopeExpansion>,
    pub(super) max_expansion_files: usize,
    pub(super) grants: &'a crate::scope_expansion::GrantLedger,
    pub(super) already_granted_paths: &'a [ScopeGlob],
    pub(super) audit: Option<(&'a dyn SessionObserver, &'a yunta_core::NodeId)>,
    pub(super) cancel: &'a CancellationToken,
    pub(super) setup: &'a SessionSetup,
    pub(super) supervision: Supervision<'a>,
}

/// What one attempt tells [`run_task`] to do next.
pub(super) enum AttemptStep {
    /// The cycle is over — record this attempt and report this outcome.
    Stop {
        record: AttemptRecord,
        outcome: TaskOutcome,
        needs_human_decision: bool,
    },
    /// Not settled — record this attempt and dispatch another.
    Again(AttemptRecord),
}

/// One attempt of the task cycle: opens a fresh session (run tools an offer
/// that degrades, never a contract), dispatches it, then verifies the result
/// the engine's own way — the agent's `Completed` never counts, only re-run
/// criteria and a clean scope diff. Returns what the adapter staged this
/// attempt alongside the step [`run_task`] acts on.
pub(super) async fn run_one_attempt(
    params: &AttemptParams<'_>,
    attempt: u32,
) -> Result<(Vec<PathBuf>, AttemptStep), TaskCycleError> {
    let (last_staged, dispatch_outcome, tokens, covered) = open_and_dispatch(params).await?;
    let &AttemptParams {
        task,
        cwd,
        memo,
        already_granted_paths,
        supervision,
        ..
    } = params;

    // A cancelled dispatch ends the cycle right here — no post-check, no
    // verdict, no retry. The attempt is on record; what the cancellation
    // means for the task is the caller's decision, because only it knows
    // which token fired.
    //
    // The token is read as well as the outcome: a `join: any` sibling can
    // win between the session closing and the verdict starting, and every
    // subprocess the verdict would run is already governed by that same
    // token — so running it would only produce a "killed before it could
    // answer" to interpret as a failure. This attempt lost; it did not
    // fail.
    let cancelled =
        matches!(dispatch_outcome, DispatchOutcome::Cancelled) || supervision.cancel.is_cancelled();
    if cancelled {
        let record = AttemptRecord {
            attempt,
            dispatch: DispatchOutcome::Cancelled,
            tokens,
            fence_breach: None,
            post_check: Vec::new(),
            scope: crate::scope::ScopeCheckResult::default(),
            succeeded: false,
            scope_expansion: None,
        };
        return Ok((
            last_staged,
            AttemptStep::Stop {
                record,
                outcome: TaskOutcome::Interrupted,
                needs_human_decision: false,
            },
        ));
    }

    let expansion_outcome = evaluate_scope_expansion(params).await?;
    let granted_paths: &[ScopeGlob] = expansion_outcome
        .as_ref()
        .filter(|outcome| outcome.decision == crate::scope_expansion::Decision::Granted)
        .map(|outcome| outcome.request.paths.as_slice())
        .unwrap_or(&[]);
    let effective_scope: Vec<ScopeGlob> = task
        .scope
        .iter()
        .cloned()
        .chain(already_granted_paths.iter().cloned())
        .chain(granted_paths.iter().cloned())
        .collect();

    let post_runs = post_check(task, cwd, memo, supervision).await?;
    // The final diff is evaluated against the declared scope plus any
    // authorized expansions — never against a denied or escalated request's
    // paths.
    // A task works in a tree of its own, so what it began with is the
    // commit that tree was made from.
    let from = crate::scope::head_tree(cwd, supervision).await?;
    let scope = audit(
        cwd,
        &from,
        &crate::run_dir::task_index(&params.setup.run_dir, &task.id),
        &effective_scope,
        &last_staged,
        supervision,
    )
    .await?;

    let criteria_green = post_runs.iter().all(|r| r.exit_code == 0);
    let succeeded = criteria_green && scope.violations.is_empty();
    let escalated = matches!(
        expansion_outcome.as_ref().map(|o| &o.decision),
        Some(crate::scope_expansion::Decision::Escalate)
    );
    // The session declared its failure won't yield to another try — captured
    // before the outcome moves into the record below.
    let non_retryable_failure = matches!(
        dispatch_outcome,
        DispatchOutcome::Failed {
            retryable: false,
            ..
        }
    );
    // The same, for a session that never reported anything at all: it
    // left no work behind and said how its process went, and the next
    // attempt would open the same session against the same
    // configuration. Captured here for the same reason.
    let session_death = match &dispatch_outcome {
        DispatchOutcome::Crashed { exit } => Some(SessionDeath {
            adapter: params.adapter.id().clone(),
            exit: exit.clone(),
        }),
        _ => None,
    };

    let record = AttemptRecord {
        attempt,
        dispatch: dispatch_outcome,
        tokens,
        fence_breach: crate::scope::fence_breach(covered.as_ref(), &scope),
        post_check: post_runs,
        scope,
        succeeded,
        scope_expansion: expansion_outcome,
    };

    if succeeded {
        return Ok((
            last_staged,
            AttemptStep::Stop {
                record,
                outcome: TaskOutcome::Done,
                needs_human_decision: escalated,
            },
        ));
    }
    // A pending human decision means no further session should spend budget
    // while the run is about to pause for it.
    if escalated {
        return Ok((
            last_staged,
            AttemptStep::Stop {
                record,
                outcome: TaskOutcome::Blocked {
                    cause: super::BlockedCause::ScopeDecisionOwed,
                },
                needs_human_decision: true,
            },
        ));
    }
    // A session that died says so instead of leaving the tail to report
    // criteria that were never run.
    if let Some(died) = session_death {
        return Ok((
            last_staged,
            AttemptStep::Stop {
                record,
                outcome: TaskOutcome::Blocked {
                    cause: super::BlockedCause::SessionDied(died),
                },
                needs_human_decision: false,
            },
        ));
    }
    // A failure the session marked non-retryable ends the cycle now: the
    // criteria were still verified above (the engine never trusts the
    // session's own verdict), and having found them unmet, another attempt
    // would only spend budget on the same dead end.
    if non_retryable_failure {
        return Ok((
            last_staged,
            AttemptStep::Stop {
                record,
                outcome: TaskOutcome::Blocked {
                    cause: super::BlockedCause::NonRetryable,
                },
                needs_human_decision: false,
            },
        ));
    }
    Ok((last_staged, AttemptStep::Again(record)))
}

/// Opens a fresh session for one attempt and drives it to a terminal
/// outcome: a per-attempt run-tools listener (its bind failure degrades to
/// no tools, recorded, never fatal), the task's brief, and the dispatch
/// itself. Returns what the adapter staged, the dispatch outcome, and the
/// tokens it spent.
async fn open_and_dispatch(
    params: &AttemptParams<'_>,
) -> Result<
    (
        Vec<PathBuf>,
        DispatchOutcome,
        TokenUsage,
        Option<yunta_core::fence::Coverage>,
    ),
    TaskCycleError,
> {
    let &AttemptParams {
        task,
        instruction,
        adapter,
        node,
        cwd,
        budget,
        profile,
        audit,
        cancel,
        setup,
        already_granted_paths,
        ..
    } = params;
    // One door for every session: the per-attempt listener (its bind
    // failure degrades to no tools, recorded, never fatal), the brief,
    // and the request itself.
    let crate::run::session_plan::OpenedSession {
        request,
        run_tools: _run_tools,
    } = crate::run::session_plan::open_session(
        setup,
        crate::run::session_plan::SessionPlan {
            node,
            task: Some(task),
            prompt: crate::run::session_plan::task_brief(instruction, task),
            cwd: cwd.to_path_buf(),
            profile,
            budget,
            granted: already_granted_paths.to_vec(),
        },
        adapter,
        audit,
    )
    .await
    .map_err(|error| match error {
        crate::run::session_plan::OpenSessionError::Audit(source) => TaskCycleError::Audit {
            task: task.id.clone(),
            source,
        },
        crate::run::session_plan::OpenSessionError::RunTools(source) => TaskCycleError::RunTools {
            task: task.id.clone(),
            source,
        },
    })?;
    let last_staged = adapter.staged_paths(&request);
    let crate::task_cycle::Dispatched {
        outcome: dispatch_outcome,
        tokens,
        fence,
    } = dispatch_session(adapter, request, cancel, audit, None)
        .await
        .map_err(|error| match error {
            DispatchError::Adapter(source) => TaskCycleError::Spawn {
                task: task.id.clone(),
                source,
            },
            DispatchError::Audit(source) => TaskCycleError::Audit {
                task: task.id.clone(),
                source,
            },
        })?;
    Ok((last_staged, dispatch_outcome, tokens, fence))
}

/// Reads the agent's own scope-expansion request from this attempt's
/// worktree (a fresh session per attempt leaves it there, not on the log)
/// and evaluates it against the node's declared mode, `within` set and cap.
/// `None` when the attempt left no request — the ordinary case.
async fn evaluate_scope_expansion(
    params: &AttemptParams<'_>,
) -> Result<Option<crate::scope_expansion::ScopeExpansionOutcome>, TaskCycleError> {
    let &AttemptParams {
        task,
        cwd,
        scope_expansion,
        max_expansion_files,
        grants,
        supervision,
        ..
    } = params;
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
