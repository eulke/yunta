//! A cycle that continues from the work a blocked task's last attempt
//! left, when a person chose that over starting again.
//!
//! The work goes back into the fresh unit as uncommitted changes and is
//! judged before any session opens, with the same judgement an
//! attempt's close makes: a task the work already closes is done
//! without spending one, one whose criterion still cannot run stops
//! there, and only a task the work does not close goes on to sessions.
//! The first cycle's pre-check already proved the criteria red; this
//! cycle has no pre-check of its own, because its tree holds the work.

use yunta_core::events::Phase;
use yunta_core::{CommitSha, ScopeGlob};

use super::attempt::AttemptParams;
use super::judge::{judge, Judgement, Work};
use super::record::Recorder;
use super::{BlockedCause, CriterionRun, TaskCycleError, TaskOutcome};
use crate::worktree::{carry_work, Carried};

/// What judging the carried work settled.
pub(super) enum Carry {
    /// The cycle ends here, before any session.
    Settled {
        outcome: TaskOutcome,
        last_check: Option<yunta_core::Seq>,
    },
    /// The work does not close the task: sessions take it from here.
    Unsettled { last_check: Option<yunta_core::Seq> },
}

/// Puts the work at `left` back into this cycle's unit and judges it.
pub(super) async fn continue_from(
    params: &AttemptParams<'_>,
    recorder: Recorder<'_>,
    left: &CommitSha,
) -> Result<Carry, TaskCycleError> {
    let &AttemptParams {
        task,
        unit,
        supervision,
        ..
    } = params;
    let carried = carry_work(unit, left, supervision)
        .await
        .map_err(|source| TaskCycleError::Carry {
            task: task.id.clone(),
            source: Box::new(source),
        })?;
    if let Carried::NoLongerApplies { paths } = carried {
        return Ok(Carry::Settled {
            last_check: recorder
                .criteria(Phase::Post, std::slice::from_ref(&stopped_on(left, &paths)))
                .await?,
            outcome: TaskOutcome::Blocked {
                cause: BlockedCause::CarriedWorkNoLongerApplies { paths },
            },
        });
    }
    // The tests the task is held to, as its spec has them, whatever the
    // carried work held of them.
    super::spec::write(task, unit, params.setup).await?;
    judge_in_place(params, recorder).await
}

/// Judges the work already in this cycle's unit — what a cycle resuming
/// the session that left it finds there — before any session opens. The
/// answer the session waited for may be all the work needed.
pub(super) async fn judge_in_place(
    params: &AttemptParams<'_>,
    recorder: Recorder<'_>,
) -> Result<Carry, TaskCycleError> {
    let judgement = judged(params).await?;
    let last_check = recorder.criteria(Phase::Post, &judgement.criteria).await?;
    recorder.scope(&judgement.scope).await?;
    Ok(match settled(&judgement) {
        Some(outcome) => Carry::Settled {
            outcome,
            last_check,
        },
        None => Carry::Unsettled { last_check },
    })
}

/// The carried work, judged as an attempt's close judges its own: the
/// criteria on the unit's tree, and its diff against the scope the task
/// declared plus what the log granted it.
async fn judged(params: &AttemptParams<'_>) -> Result<Judgement, TaskCycleError> {
    let &AttemptParams {
        task,
        unit,
        memo,
        setup,
        already_granted_paths,
        supervision,
        ..
    } = params;
    let scope: Vec<ScopeGlob> = task
        .scope
        .iter()
        .chain(already_granted_paths)
        .cloned()
        .collect();
    judge(
        task,
        crate::scope::Ceiling {
            scope: &scope,
            deny: params.denied,
        },
        Work {
            unit,
            index: &crate::run_dir::index_for(&setup.run_dir, &unit.who),
            staged: &[],
        },
        memo,
        supervision,
    )
    .await
}

/// What the carried work settles without a session: a criterion that
/// still cannot run blocks the task, work that closes it finishes it,
/// and anything else is for the sessions to take on.
fn settled(judgement: &Judgement) -> Option<TaskOutcome> {
    let unrunnable: Vec<CriterionRun> = judgement
        .criteria
        .iter()
        .filter(|run| run.could_not_run().is_some())
        .cloned()
        .collect();
    if !unrunnable.is_empty() {
        return Some(TaskOutcome::Blocked {
            cause: BlockedCause::Unrunnable { runs: unrunnable },
        });
    }
    judgement.closes().then_some(TaskOutcome::Done)
}

/// The check a pick that stopped on conflicts is recorded as — the way a
/// replay that stopped on conflicts is: the git command and the paths it
/// stopped on, as a criterion that did not pass.
fn stopped_on(left: &CommitSha, paths: &[std::path::PathBuf]) -> CriterionRun {
    let listed: Vec<String> = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    CriterionRun {
        cmd: format!("git cherry-pick {left}: conflicts in {}", listed.join(", ")),
        output: None,
        exit_code: 1,
        is_guard: false,
        reused: false,
        duration_ms: None,
    }
}
