//! One attempt of a task: the session it opens, the scope check on what
//! it changed, and what it tells `run_task` to do next.

use std::path::PathBuf;
use yunta_core::ScopeGlob;

use tokio_util::sync::CancellationToken;
use yunta_core::events::{DeviationDeclaredPayload, Phase, SessionDeath};
use yunta_core::port::{Adapter, Budget, PermissionProfile};
use yunta_core::{NonEmpty, Task};

use super::criteria::Memo;
use super::judge::{judge, Judgement, Work};
use super::record::Recorder;
use super::session::{dispatch_session, DispatchError, SessionObserver, SessionSetup};
use super::{AttemptRecord, DispatchOutcome, TaskCycleError, TaskOutcome};
use crate::process::Supervision;

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
    pub(super) grants: &'a crate::scope_expansion::GrantLedger,
    /// Every path granted before this attempt: on the log when the cycle
    /// began, and what the engine granted in the cycle's earlier attempts.
    pub(super) already_granted_paths: &'a [ScopeGlob],
    /// What the project denies to every run: never written, never
    /// granted.
    pub(super) denied: &'a [ScopeGlob],
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
    /// Not settled, and nothing changed that another session could use:
    /// the same task, the same tree, the same evidence this one already
    /// had. Record it; the task blocks and a person decides.
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
    let (
        last_staged,
        super::Dispatched {
            outcome: dispatch_outcome,
            tokens,
            fence: covered,
            session,
        },
        declared,
    ) = open_and_dispatch(params).await?;
    let &AttemptParams {
        task,
        unit,
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
        // Nothing was checked, and the log says so in the attempt's own
        // place: an empty post-check and an empty scope audit.
        let scope = crate::scope::ScopeCheckResult::default();
        let recorded = recorder.criteria(Phase::Post, &[]).await?;
        recorder.scope(&scope).await?;
        let record = AttemptRecord {
            attempt,
            session,
            dispatch: DispatchOutcome::Cancelled,
            tokens,
            fence_breach: None,
            post_check: Vec::new(),
            scope,
            succeeded: false,
            scope_expansion: None,
            recorded,
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

    let expansion_outcome = super::expansion::evaluate_scope_expansion(params).await?;
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

    // The final diff is evaluated against the declared scope plus any
    // authorized expansions — never against a denied or escalated request's
    // paths.
    let judgement = judge(
        task,
        crate::scope::Ceiling {
            scope: &effective_scope,
            deny: params.denied,
        },
        Work {
            unit,
            index: &crate::run_dir::index_for(&params.setup.run_dir, &unit.who),
            staged: &last_staged,
        },
        memo,
        supervision,
    )
    .await?;
    let succeeded = judgement.closes();
    let Judgement {
        criteria: post_runs,
        scope,
    } = judgement;
    // On the log before anything else happens to this task, so the next
    // attempt's session can read why this one did not close.
    let recorded = recorder.criteria(Phase::Post, &post_runs).await?;
    recorder.scope(&scope).await?;

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
        session,
        dispatch: dispatch_outcome,
        tokens,
        fence_breach: crate::scope::fence_breach(covered.as_ref(), &scope),
        post_check: post_runs,
        scope,
        succeeded,
        scope_expansion: expansion_outcome,
        recorded,
    };

    // A departure from the plan its session declared keeps the task open
    // whatever its criteria say: a person answers it first. A scope
    // request owed an answer goes first, since that answer resumes the
    // same session on the same work.
    if let Some(deviations) = NonEmpty::new(declared).filter(|_| !escalated) {
        return Ok((
            last_staged,
            AttemptStep::Stop {
                record,
                outcome: TaskOutcome::Blocked {
                    cause: super::BlockedCause::DeviationOwed { deviations },
                },
                needs_human_decision: true,
            },
        ));
    }
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
    // A criterion that never answered is the environment's, not the
    // work's: the next session would change the tree, never whether the
    // engine can run the command. The cycle stops here and says which.
    let unrunnable: Vec<super::CriterionRun> = record
        .post_check
        .iter()
        .filter(|run| run.could_not_run().is_some())
        .cloned()
        .collect();
    if !unrunnable.is_empty() {
        return Ok((
            last_staged,
            AttemptStep::Stop {
                record,
                outcome: TaskOutcome::Blocked {
                    cause: super::BlockedCause::Unrunnable { runs: unrunnable },
                },
                needs_human_decision: false,
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
    // A grant the engine made itself is the one thing that changes what
    // the next session can do: the fence refused the write in this one.
    let granted = record
        .scope_expansion
        .as_ref()
        .is_some_and(|outcome| outcome.decision == crate::scope_expansion::Decision::Granted);
    Ok((
        last_staged,
        match granted {
            true => AttemptStep::Again(record),
            false => AttemptStep::Unmet(record),
        },
    ))
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
        super::Dispatched,
        Vec<DeviationDeclaredPayload>,
    ),
    TaskCycleError,
> {
    let &AttemptParams {
        task,
        instruction,
        adapter,
        node,
        unit,
        budget,
        profile,
        audit,
        cancel,
        setup,
        already_granted_paths,
        supervision,
        resume,
        ..
    } = params;
    let cwd = unit.worktree.as_path();
    // Kept apart from the session's tools, which the session outlives
    // here: what it declared is what this attempt's close answers for.
    let deviations: std::sync::Arc<std::sync::Mutex<Vec<DeviationDeclaredPayload>>> =
        Default::default();
    // What this session's tools read and judge: the task the cycle
    // holds, the scope it is held to — declared plus everything granted
    // before this attempt — and the unit it works in. A check
    // stages its diff through an index of its own, never the close's.
    let access = std::sync::Arc::new(crate::run_tools::TaskAccess {
        task: task.clone(),
        scope: task
            .scope
            .iter()
            .chain(already_granted_paths)
            .cloned()
            .collect(),
        denied: params.denied.to_vec(),
        unit: unit.clone(),
        index: crate::run_dir::index_for(&setup.run_dir, &unit.who).with_extension("check"),
        cancel: supervision.cancel.clone(),
        staged: Default::default(),
        checks: Default::default(),
        plan: setup.plan.clone(),
        deviations: deviations.clone(),
    });
    // One door for every session: the per-attempt listener (mandatory
    // for a task session, which reads its task through it), the brief,
    // and the request itself.
    let crate::run::session_plan::OpenedSession { request, run_tools } =
        crate::run::session_plan::open_session(
            setup,
            crate::run::session_plan::SessionPlan {
                node,
                task: Some(access),
                prompt: crate::run::session_plan::task_brief(instruction, task),
                cwd: cwd.to_path_buf(),
                profile,
                budget,
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
            crate::run::session_plan::OpenSessionError::RunTools(source) => {
                TaskCycleError::RunTools {
                    task: task.id.clone(),
                    source,
                }
            }
        })?;
    let last_staged = adapter.staged_paths(&request);
    // A session picked back up is told the answer to what it asked, not
    // the brief again; the brief is what a fresh session gets when the
    // adapter cannot pick the conversation up.
    let brief = request.prompt.clone();
    let request = match resume {
        Some(continuing) => crate::run::session_plan::with_prompt(
            request,
            crate::run_tools::continuation_notice(
                run_tools.as_ref(),
                &continuing.answer,
                crate::run_tools::Asker::Task,
            ),
        ),
        None => request,
    };
    let opening = crate::task_cycle::session::Opening {
        task: Some(&task.id),
        resume: resume.map(|continuing| crate::task_cycle::session::Resume {
            session: &continuing.session,
            fresh_prompt: Some(&brief),
        }),
    };
    let dispatched = dispatch_session(adapter, request, cancel, audit, opening)
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
    let declared = std::mem::take(&mut *deviations.lock().unwrap_or_else(|e| e.into_inner()));
    Ok((last_staged, dispatched, declared))
}
