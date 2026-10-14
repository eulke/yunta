//! One attempt of a task: the session it opens, the scope check on what
//! it changed, and what it tells `run_task` to do next.

use std::path::PathBuf;
use yunta_core::ScopeGlob;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{
    AccountLimit, CriterionResult, CriterionUnpassablePayload, FailureCause, LimitSource, Phase,
    SessionDeath, SessionRefusal, TaskCause,
};
use yunta_core::expansion::Decision;
use yunta_core::port::{Adapter, Budget, PermissionProfile};
use yunta_core::protected::{Closing, Protected};
use yunta_core::{AdapterId, SessionId, Task};

use super::criteria::could_not_run;
use super::judge::{judge, scope_in_force, JudgeEnv, Judgement, Refusals, Settle, Sitting, Work};
use super::memo::Memo;
use super::open::open_and_dispatch;
use super::record::{Checked, Recorder};
use super::session::{SessionObserver, SessionSetup};
use super::{AttemptRecord, DispatchOutcome, TaskCycleError, TaskOutcome};
use crate::process::Supervision;
use crate::run_tools::TaskAccess;
use crate::scope_expansion::{consume_request, read_request, ScopeExpansionOutcome};

/// Everything one attempt of [`run_task`] reads: the per-cycle context that
/// never changes between attempts, so an attempt takes just this and its
/// number.
pub(super) struct AttemptParams<'a> {
    pub(super) task: &'a Task,
    pub(super) instruction: &'a str,
    pub(super) adapter: &'a dyn Adapter,
    pub(super) node: &'a yunta_core::Node,
    pub(super) unit: &'a crate::worktree::Unit,
    pub(super) budget: Budget,
    pub(super) memo: &'a Memo,
    pub(super) profile: PermissionProfile,
    pub(super) scope_expansion: Option<&'a yunta_core::ScopeExpansion>,
    pub(super) max_expansion_files: usize,
    pub(super) grants: &'a crate::scope_expansion::GrantWindow,
    /// Every path granted before this attempt: on the log when the cycle
    /// began, and what the engine granted in the cycle's earlier attempts.
    pub(super) already_granted_paths: &'a [ScopeGlob],
    /// What the other tasks of the batch may write.
    pub(super) beside: &'a [ScopeGlob],
    /// What no session of the run may write, as this task's lifted set
    /// judges it: never written, never granted.
    pub(super) protected: &'a Protected,
    /// The session this attempt picks back up, and the answer it is told,
    /// instead of opening a fresh one.
    pub(super) resume: Option<&'a super::Continuing>,
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
    /// Not settled, and the engine widened the task's scope during this
    /// attempt — record it and dispatch another, which is the first that
    /// may write what the grant allows.
    Again(AttemptRecord),
    /// Not settled, and no grant: record it. `run_task` decides between
    /// another attempt and the end of the cycle.
    Unmet(AttemptRecord),
}

/// One attempt of the task cycle: opens a fresh session (run tools an offer
/// that degrades, never a contract), dispatches it, then verifies the result
/// the engine's own way — the agent's `Completed` never counts, only re-run
/// criteria and a clean scope diff. Returns what the adapter staged this
/// attempt alongside the step [`run_task`] acts on.
pub(super) async fn run_one_attempt(
    params: &AttemptParams<'_>,
    recorder: Recorder<'_>,
    attempt: u32,
) -> Result<(Vec<PathBuf>, AttemptStep), TaskCycleError> {
    let (last_staged, dispatched, access) = open_and_dispatch(params, attempt).await?;
    // The session the attempt ends in is the one a next attempt picks back up.
    let session = dispatched.sessions.last().cloned();
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
    let cancelled = matches!(dispatched.outcome, DispatchOutcome::Cancelled)
        || params.supervision.cancel.is_cancelled();
    if cancelled {
        return interrupted(&recorder, attempt, session, dispatched.tokens, last_staged).await;
    }

    let Some(judgement) =
        judge_attempt(params, &access, &dispatched.sessions, &last_staged).await?
    else {
        return interrupted(&recorder, attempt, session, dispatched.tokens, last_staged).await;
    };
    verdict(
        params,
        recorder,
        attempt,
        dispatched,
        last_staged,
        judgement,
    )
    .await
}

/// Records what the judgement found on the log, then the attempt as a record,
/// and returns the step the cycle takes next from it.
async fn verdict(
    params: &AttemptParams<'_>,
    recorder: Recorder<'_>,
    attempt: u32,
    dispatched: super::Dispatched,
    last_staged: Vec<PathBuf>,
    judgement: Judgement,
) -> Result<(Vec<PathBuf>, AttemptStep), TaskCycleError> {
    let succeeded = judgement.closes();
    let withdrawn = withdrawn_of(succeeded, judgement.request.as_ref());
    let Judgement {
        criteria: post_runs,
        waiting,
        scope,
        request: expansion_outcome,
        unpassable,
        ..
    } = judgement;
    // On the log before anything else happens to this task, so the next
    // attempt's session can read why this one did not close.
    let Checked {
        seq: recorded,
        results: post_results,
    } = recorder.post_check(&post_runs, &waiting).await?;
    recorder.scope(&scope).await?;
    // The same, for a session that never reported anything at all: it
    // left no work behind and said how its process went, and the next
    // attempt would open the same session against the same
    // configuration. Captured here for the same reason.
    let session_death = session_death_of(&dispatched.outcome, params.adapter.id());
    let record = AttemptRecord {
        attempt,
        session: dispatched.sessions.last().cloned(),
        dispatch: dispatched.outcome,
        tokens: dispatched.tokens,
        fence_breach: crate::scope::fence_breach(
            dispatched.fence.as_ref(),
            dispatched.confinement.as_ref(),
            &scope,
        ),
        post_check: post_runs,
        post_results,
        scope,
        succeeded,
        scope_expansion: expansion_outcome,
        withdrawn,
        recorded,
    };
    if record.request_settled() {
        consume_request(params.unit.worktree.as_path())
            .await
            .map_err(|source| TaskCycleError::ScopeExpansion {
                task: params.task.id.clone(),
                source,
            })?;
    }
    step_of(params, last_staged, record, unpassable, session_death).await
}

/// Judges the work an attempt's session left against the scope in force and
/// the request the session made. `None` when a cancellation landed while the
/// work was judged: nothing a cut judgement answered is a verdict on the task.
async fn judge_attempt(
    params: &AttemptParams<'_>,
    access: &TaskAccess,
    sessions: &[SessionId],
    staged: &[PathBuf],
) -> Result<Option<Judgement>, TaskCycleError> {
    // The request the session left is read here and decided by the judgement
    // on the tree the session left. Nothing is consumed until it is settled.
    let request = read_request(params.unit.worktree.as_path())
        .await
        .map_err(|source| TaskCycleError::ScopeExpansion {
            task: params.task.id.clone(),
            source,
        })?;
    let declared = access.declared.lock().await.clone();
    let in_force = scope_in_force(params.task, params.already_granted_paths);
    let refused = refused_of(params.audit, sessions);
    // The final diff is evaluated against the declared scope plus any
    // authorized expansions — never against a denied or escalated request's
    // paths.
    let judged = judge(
        Sitting {
            work: Work {
                checkout: params.unit.worktree.as_path(),
                from: &params.unit.from,
                staged,
            },
            task: Some(params.task),
            scope: &in_force,
            request: request.as_ref(),
            refused,
            window: Some(params.grants),
            bounds: Some(access.scope_bounds()),
            declared: declared.as_ref(),
            documents: None,
            env: JudgeEnv {
                memo: params.memo,
                protected: params.protected,
                closing: Closing::Every,
                supervision: params.supervision,
            },
        },
        Settle::Commit,
    )
    .await;
    // A cancellation that lands while the work is judged — a check waiting
    // for the measurement, a criterion running — cuts the attempt the same
    // way: nothing it answered is a verdict on the task.
    if params.supervision.cancel.is_cancelled() {
        return Ok(None);
    }
    judged.map(Some)
}

/// Where the judgement reads the refused writes a red criterion may locate: the
/// observer of an audited node, with the sessions the attempt opened.
fn refused_of<'a>(
    audit: Option<(&'a dyn SessionObserver, &'a yunta_core::NodeId)>,
    sessions: &'a [SessionId],
) -> Refusals<'a> {
    match audit {
        Some((observer, node)) => Refusals::Observer {
            observer,
            node,
            sessions,
        },
        None => Refusals::None,
    }
}

/// The step an attempt tells the cycle to take next, from what its judgement
/// and its dispatch left: the cycle closes, stops on a cause no retry can pass,
/// or goes on to another attempt.
async fn step_of(
    params: &AttemptParams<'_>,
    last_staged: Vec<PathBuf>,
    record: AttemptRecord,
    unpassable: Option<CriterionUnpassablePayload>,
    session_death: Option<SessionDeath>,
) -> Result<(Vec<PathBuf>, AttemptStep), TaskCycleError> {
    if record.succeeded {
        return Ok(stop(last_staged, record, TaskOutcome::Done));
    }
    // A criterion the session declared impossible ends the cycle before any
    // retry: no session on this tree could pass it, and no person decides it.
    if let Some(declared) = unpassable {
        let cause = TaskCause::CriterionUnpassable { declared };
        return Ok(stop(last_staged, record, TaskOutcome::Blocked { cause }));
    }
    // A criterion that never answered is the environment's, not the
    // work's: the next session would change the tree, never whether the
    // engine can run the command. The cycle stops here and says which.
    let unrunnable: Vec<CriterionResult> = record
        .post_results
        .iter()
        .filter(|result| could_not_run(result.exit_code).is_some())
        .cloned()
        .collect();
    if !unrunnable.is_empty() {
        let cause = TaskCause::Unrunnable { runs: unrunnable };
        return stop_unclosed(params.unit, params.task, last_staged, record, cause).await;
    }
    // A session that died says so instead of leaving the tail to report
    // criteria that were never run.
    if let Some(died) = session_death {
        let cause = TaskCause::SessionDied(died);
        return stop_unclosed(params.unit, params.task, last_staged, record, cause).await;
    }
    // A session that refused the request, or whose account reached its
    // limit, ends the cycle here, with the criteria judged above: the
    // engine never trusts the session's own verdict, and no other session
    // is opened on the same request.
    if let Some(cause) = failure_stop(params.adapter.id(), &record.dispatch) {
        return stop_unclosed(params.unit, params.task, last_staged, record, cause).await;
    }
    // A grant the engine made itself is the one thing that changes what
    // the next session can do: the fence refused the write in this one.
    let granted = record
        .scope_expansion
        .as_ref()
        .is_some_and(|outcome| matches!(outcome.decision, Decision::Grant { .. }));
    Ok((
        last_staged,
        match granted {
            true => AttemptStep::Again(record),
            false => AttemptStep::Unmet(record),
        },
    ))
}

/// The death of a session that ended without reporting anything: it left no
/// work behind and said how its process went, and the next attempt would open
/// the same session against the same configuration. `None` for every other end.
fn session_death_of(dispatch: &DispatchOutcome, adapter: &AdapterId) -> Option<SessionDeath> {
    match dispatch {
        DispatchOutcome::Crashed { exit } => Some(SessionDeath {
            adapter: adapter.clone(),
            exit: exit.clone(),
        }),
        _ => None,
    }
}

/// The cause a session that ended in a failure stops the cycle with: its
/// refusal, or its account's limit, with the message the adapter gave. `None`
/// for every other end.
fn failure_stop(adapter: &AdapterId, dispatch: &DispatchOutcome) -> Option<TaskCause> {
    let DispatchOutcome::Failed { error, cause } = dispatch else {
        return None;
    };
    let message = yunta_core::describe(error);
    Some(match cause {
        FailureCause::Refused(cause) => TaskCause::SessionRefused(SessionRefusal {
            adapter: adapter.clone(),
            cause: cause.clone(),
            message,
        }),
        FailureCause::Limit { resets_at } => TaskCause::SessionLimited(AccountLimit {
            source: LimitSource::Session {
                adapter: adapter.clone(),
            },
            resets_at: *resets_at,
            message,
        }),
    })
}

/// The paths an escalation leaves unanswered when its task closes: nobody
/// decides it once the task is done, so it is withdrawn. `None` for any
/// other request, and for an attempt that did not close.
fn withdrawn_of(
    succeeded: bool,
    outcome: Option<&ScopeExpansionOutcome>,
) -> Option<Vec<ScopeGlob>> {
    match outcome.map(|outcome| &outcome.decision) {
        Some(Decision::Escalate { paths, .. }) if succeeded => Some(paths.clone()),
        _ => None,
    }
}

/// The step that ends the cycle with `outcome`. No person decides an attempt
/// that stops here, so the step never asks for one.
fn stop(
    last_staged: Vec<PathBuf>,
    record: AttemptRecord,
    outcome: TaskOutcome,
) -> (Vec<PathBuf>, AttemptStep) {
    let step = AttemptStep::Stop {
        record,
        outcome,
        needs_human_decision: false,
    };
    (last_staged, step)
}

/// The step that ends the cycle without closing its task, for a cause the
/// attempt cannot be retried past. A request the attempt left pending is owed
/// a person's decision there, as it is when a cycle ends unclosed: the request
/// leaves the checkout and the cycle reports `ScopeDecisionOwed`.
async fn stop_unclosed(
    unit: &crate::worktree::Unit,
    task: &Task,
    last_staged: Vec<PathBuf>,
    record: AttemptRecord,
    cause: TaskCause,
) -> Result<(Vec<PathBuf>, AttemptStep), TaskCycleError> {
    if !record.escalated() {
        return Ok(stop(last_staged, record, TaskOutcome::Blocked { cause }));
    }
    consume_request(unit.worktree.as_path())
        .await
        .map_err(|source| TaskCycleError::ScopeExpansion {
            task: task.id.clone(),
            source,
        })?;
    let step = AttemptStep::Stop {
        record,
        outcome: TaskOutcome::Blocked {
            cause: TaskCause::ScopeDecisionOwed,
        },
        needs_human_decision: true,
    };
    Ok((last_staged, step))
}

/// An attempt a cancellation cut: nothing was checked, and the log says so in
/// the attempt's own place — an empty post-check and an empty scope audit.
/// What the cancellation means for the task is the caller's decision, because
/// only it knows which token fired.
async fn interrupted(
    recorder: &Recorder<'_>,
    attempt: u32,
    session: Option<yunta_core::SessionId>,
    tokens: yunta_core::events::TokenUsage,
    last_staged: Vec<PathBuf>,
) -> Result<(Vec<PathBuf>, AttemptStep), TaskCycleError> {
    let scope = crate::scope::ScopeCheckResult::default();
    let Checked {
        seq: recorded,
        results: post_results,
    } = recorder.criteria(Phase::Post, &[]).await?;
    recorder.scope(&scope).await?;
    let record = AttemptRecord {
        attempt,
        session,
        dispatch: DispatchOutcome::Cancelled,
        tokens,
        fence_breach: None,
        post_check: Vec::new(),
        post_results,
        scope,
        succeeded: false,
        scope_expansion: None,
        withdrawn: None,
        recorded,
    };
    Ok(stop(last_staged, record, TaskOutcome::Interrupted))
}
